use axum::{
    extract::{Form, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::auth::{AuthError, AuthState, SESSION_COOKIE};
use crate::markdown::render_markdown;
use crate::notes::{self, Note};
use crate::pages::{self, Base};
use crate::themes::get_theme;

#[derive(Clone)]
pub struct AppState {
    pub auth: Arc<AuthState>,
    /// Path prefix when served behind a reverse proxy subdirectory.
    /// Empty means mounted at the root, which is the default and the only
    /// configuration most installs need.
    pub base_path: Arc<str>,
}

impl AppState {
    /// Joins an app-absolute path onto the configured base path.
    ///
    /// Used for every `Location` header. HTML links and form actions get the
    /// same treatment via `Base` in `pages.rs`.
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_path, path)
    }
}

/// Normalizes a user-supplied base path.
///
/// `""` and `"/"` both mean "mounted at the root". Anything else keeps a
/// leading slash and loses any trailing one, so `--base-path forgejo/`,
/// `/forgejo`, and `/forgejo/` all normalize to `/forgejo`.
pub fn normalize_base_path(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_root_both_mean_no_prefix() {
        assert_eq!(normalize_base_path(""), "");
        assert_eq!(normalize_base_path("/"), "");
        assert_eq!(normalize_base_path("  "), "");
    }

    #[test]
    fn prefix_keeps_one_leading_slash_and_drops_trailing_ones() {
        assert_eq!(normalize_base_path("/forgejo"), "/forgejo");
        assert_eq!(normalize_base_path("forgejo"), "/forgejo");
        assert_eq!(normalize_base_path("/forgejo/"), "/forgejo");
        assert_eq!(normalize_base_path("forgejo/"), "/forgejo");
        assert_eq!(normalize_base_path("/forgejo///"), "/forgejo");
        assert_eq!(normalize_base_path("  /forgejo  "), "/forgejo");
    }

    #[test]
    fn nested_prefixes_survive() {
        assert_eq!(normalize_base_path("/a/b"), "/a/b");
        assert_eq!(normalize_base_path("a/b/"), "/a/b");
    }

    #[test]
    fn url_joins_prefix_onto_app_paths() {
        let root = AppState {
            auth: Arc::new(AuthState::with_secret_path("/nonexistent-test-secret")),
            base_path: "".into(),
        };
        assert_eq!(root.url("/"), "/");
        assert_eq!(root.url("/about"), "/about");

        let mounted = AppState {
            auth: root.auth.clone(),
            base_path: "/forgejo".into(),
        };
        assert_eq!(mounted.url("/"), "/forgejo/");
        assert_eq!(mounted.url("/about"), "/forgejo/about");
        assert_eq!(mounted.url("/notes/abc"), "/forgejo/notes/abc");
    }

    #[test]
    fn cookie_path_is_scoped_or_default() {
        assert_eq!(cookie_path(""), "/");
        assert_eq!(cookie_path("/forgejo"), "/forgejo");
    }
}

pub fn router(state: AppState) -> Router {
    // Auth routes must be reachable while logged out, so they are merged
    // outside the protected layer. TOTP setup is deliberately absent: it
    // completes in the terminal before the listener ever binds, so there is
    // no window in which an unauthenticated peer could claim the device.
    let public = Router::new()
        .route("/login", get(login_page).post(login_submit))
        .route("/logout", post(logout));

    let protected = Router::new()
        .route("/", get(index))
        .route("/notes", post(create_note_route))
        .route(
            "/notes/:id",
            get(show_note_route).post(update_or_delete_note_route),
        )
        .route("/preview", post(preview_route))
        .route("/theme", post(theme_route))
        .route("/about", get(about_route))
        .route("/export", get(export_route))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    public.merge(protected).with_state(state)
}

/// Alpine.js, embedded at compile time.
///
/// This is the only asset served as an external file. `style.css` and
/// `app.js` are already inlined into every page with `include_str!`, so
/// embedding this one file makes the binary fully self-contained: there is
/// no way for a deployed instance to be missing its JavaScript because the
/// working directory did not have a `static/` folder next to it.
const ALPINE_JS: &[u8] = include_bytes!("../static/alpine.min.js");

/// Assembles the full application, including static assets.
///
/// The static route is app-absolute at `/static` regardless of the base
/// path. A reverse proxy strips the base path before forwarding, so the
/// browser's `{base}/static/alpine.min.js` arrives here as
/// `/static/alpine.min.js`.
///
/// The invariant: *emitted* URLs carry the base path, *received* paths never
/// do. Mounting this at `{base}/static` makes the asset 404, which leaves
/// `x-data="app()"` uninitialised and every Alpine directive — including the
/// menu dropdowns — silently dead.
///
/// This lives here rather than in `main.rs` so tests exercise the same
/// assembly the binary uses, rather than a hand-rolled copy of it.
pub fn app(state: AppState) -> Router {
    Router::new()
        .merge(router(state))
        // Outside the auth gate: the login page is styled by this same bundle
        // and must render before anyone can authenticate.
        .route("/static/alpine.min.js", get(serve_alpine_js))
}

async fn serve_alpine_js() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            // The bytes are fixed at compile time, so they can never change
            // for a given binary. Safe to cache indefinitely.
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        ALPINE_JS,
    )
        .into_response()
}

/// Rejects unauthenticated requests before they reach any note data.
///
/// This is a hard gate, not a redirect-to-login nicety: unauthenticated
/// callers get 401 and never observe note content, titles, or counts.
async fn require_auth(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if let Some(token) = read_cookie(&headers, SESSION_COOKIE) {
        if state.auth.validate_session(&token) {
            return next.run(request).await;
        }
    }

    // API-ish endpoints stay JSON-ish/short rather than redirecting, so a
    // stale fetch() in the page fails loudly instead of silently receiving
    // the login HTML.
    if request.uri().path() == "/preview" {
        return (StatusCode::UNAUTHORIZED, "Authentication required").into_response();
    }

    // Setup is handled in the terminal before the listener binds, so the
    // only reachable unauthenticated state is "not logged in".
    Redirect::to(&state.url("/login")).into_response()
}

fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(';'))
        .find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            (key == name).then(|| value.to_string())
        })
}

fn session_cookie(token: &str, base_path: &str) -> String {
    // HttpOnly keeps the token out of reach of any injected script;
    // SameSite=Strict limits cross-site replay; Secure is omitted so the app
    // also works over plain HTTP on a trusted LAN.
    //
    // `Path` is scoped to the base path so the token is not sent to sibling
    // apps sharing the host. When mounted at the root this is "/" (RFC 6265
    // treats a bare path as the default-path, i.e. the whole origin).
    let path = if base_path.is_empty() { "/" } else { base_path };
    format!("{SESSION_COOKIE}={token}; Path={path}; HttpOnly; SameSite=Strict; Max-Age=3600")
}

fn get_theme_from_cookie(headers: &HeaderMap) -> String {
    read_cookie(headers, "theme").unwrap_or_else(|| "Dark".to_string())
}

#[derive(Deserialize)]
struct CodeForm {
    code: String,
}

async fn login_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let theme = get_theme_from_cookie(&headers);

    // Setup happens in the terminal before the listener binds, so an
    // unprovisioned secret file can only mean the operator deleted it while
    // the server was running. Say so plainly rather than looping them
    // through a login form that can never succeed.
    if state.auth.needs_setup() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "This device is not set up yet. Delete nothing, restart the server, \
             and complete setup in the terminal.",
        )
            .into_response();
    }

    let base = Base::new(&state.base_path);
    Html(pages::render_auth_page(&theme, None, &base).into_string()).into_response()
}

async fn login_submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<CodeForm>,
) -> Response {
    let theme = get_theme_from_cookie(&headers);
    let base = Base::new(&state.base_path);
    match state.auth.verify_login(&form.code) {
        Ok(token) => {
            let mut res = Redirect::to(&state.url("/")).into_response();
            res.headers_mut().insert(
                header::SET_COOKIE,
                session_cookie(&token, &state.base_path).parse().unwrap(),
            );
            res
        }
        Err(AuthError::InvalidCode) => Html(
            pages::render_auth_page(
                &theme,
                Some("That code was not accepted. Check your authenticator and try again."),
                &base,
            )
            .into_string(),
        )
        .into_response(),
        Err(AuthError::NotProvisioned) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "This device is not set up yet. Restart the server and complete setup in the terminal.",
        )
            .into_response(),
        Err(e) => Html(pages::render_auth_page(&theme, Some(&e.to_string()), &base).into_string())
            .into_response(),
    }
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = read_cookie(&headers, SESSION_COOKIE) {
        // Drop the token server-side so the cookie clearing is not merely
        // cosmetic: a captured copy of the token stops working immediately.
        state.auth.destroy_session(&token);
    }
    let mut res = Redirect::to(&state.url("/login")).into_response();
    res.headers_mut().insert(
        header::SET_COOKIE,
        format!(
            "{SESSION_COOKIE}=; Path={}; HttpOnly; SameSite=Strict; Max-Age=0",
            cookie_path(&state.base_path)
        )
        .parse()
        .unwrap(),
    );
    res
}

fn cookie_path(base_path: &str) -> &str {
    if base_path.is_empty() {
        "/"
    } else {
        base_path
    }
}

#[derive(Deserialize)]
struct IndexQuery {
    q: Option<String>,
}

async fn index(
    State(state): State<AppState>,
    Query(query): Query<IndexQuery>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let theme = get_theme_from_cookie(&headers);
    let all_notes = notes::list_notes()?;

    let notes: Vec<Note> = if let Some(ref q) = query.q {
        let q = q.to_lowercase();
        all_notes
            .into_iter()
            .filter(|n| {
                n.title.to_lowercase().contains(&q) || n.content.to_lowercase().contains(&q)
            })
            .collect()
    } else {
        all_notes
    };

    let base = Base::new(&state.base_path);
    let html = pages::render_app(&theme, &notes, None, query.q.as_deref(), None, &base);
    Ok(Html(html.into_string()))
}

async fn show_note_route(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<IndexQuery>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let theme = get_theme_from_cookie(&headers);
    let note = notes::load_note(&id)?;
    let all_notes = notes::list_notes()?;

    let notes: Vec<Note> = if let Some(ref q) = query.q {
        let q = q.to_lowercase();
        all_notes
            .into_iter()
            .filter(|n| {
                n.title.to_lowercase().contains(&q) || n.content.to_lowercase().contains(&q)
            })
            .collect()
    } else {
        all_notes
    };

    let base = Base::new(&state.base_path);
    let html = pages::render_app(&theme, &notes, Some(&note), query.q.as_deref(), None, &base);
    Ok(Html(html.into_string()))
}

#[derive(Deserialize)]
struct NoteForm {
    title: Option<String>,
    content: Option<String>,
    #[serde(default)]
    _method: String,
}

async fn create_note_route(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let note = notes::create_note("Untitled", "")?;
    Ok(Redirect::to(&state.url(&format!("/notes/{}", note.id))))
}

async fn update_or_delete_note_route(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(form): Form<NoteForm>,
) -> Result<impl IntoResponse, AppError> {
    match form._method.as_str() {
        "delete" => {
            notes::delete_note(&id)?;
            Ok(Redirect::to(&state.url("/")).into_response())
        }
        _ => {
            let title = form.title.unwrap_or_else(|| "Untitled".to_string());
            let content = form.content.unwrap_or_default();
            notes::update_note(&id, title, content)?;
            Ok(Redirect::to(&state.url(&format!("/notes/{id}"))).into_response())
        }
    }
}

#[derive(Deserialize)]
struct PreviewForm {
    content: String,
}

async fn preview_route(
    headers: HeaderMap,
    Form(form): Form<PreviewForm>,
) -> Result<impl IntoResponse, AppError> {
    let theme_name = get_theme_from_cookie(&headers);
    let theme = get_theme(&theme_name);
    let html = render_markdown(&form.content, theme);
    Ok(Html(pages::render_preview_fragment(&html).into_string()))
}

#[derive(Deserialize)]
struct ThemeForm {
    theme: String,
}

async fn theme_route(
    State(state): State<AppState>,
    Form(form): Form<ThemeForm>,
) -> Result<impl IntoResponse, AppError> {
    // Validate theme name
    let _ = get_theme(&form.theme);

    let mut response = Redirect::to(&state.url("/")).into_response();
    let cookie = format!(
        "theme={}; Path={}; Max-Age=31536000; SameSite=Lax",
        form.theme,
        cookie_path(&state.base_path)
    );
    response
        .headers_mut()
        .insert(axum::http::header::SET_COOKIE, cookie.parse().unwrap());
    Ok(response)
}

async fn about_route(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let theme = get_theme_from_cookie(&headers);
    let base = Base::new(&state.base_path);
    Ok(Html(pages::render_about(&theme, &base).into_string()))
}

async fn export_route(_headers: HeaderMap) -> Result<impl IntoResponse, AppError> {
    let all_notes = notes::list_notes()?;
    let mut export = String::from("# EternaLibre Notes Export\n\n");
    for note in all_notes {
        export.push_str(&format!("## {}\n\n{}\n\n---\n\n", note.title, note.content));
    }

    let mut response = Html(export).into_response();
    response.headers_mut().insert(
        axum::http::header::CONTENT_DISPOSITION,
        "attachment; filename=\"eternalibre-notes-export.md\""
            .parse()
            .unwrap(),
    );
    Ok(response)
}

#[derive(Debug)]
pub struct AppError(anyhow::Error);

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let not_found = self
            .0
            .root_cause()
            .downcast_ref::<std::io::Error>()
            .map(|e| e.kind() == std::io::ErrorKind::NotFound)
            .unwrap_or(false);

        if not_found {
            return (
                axum::http::StatusCode::NOT_FOUND,
                "Note not found".to_string(),
            )
                .into_response();
        }

        let body = format!("Internal Server Error: {}", self.0);
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
    }
}
