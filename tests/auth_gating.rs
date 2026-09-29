//! End-to-end tests for the TOTP auth gate.
//!
//! These drive the real router over HTTP (through `tower`'s `oneshot`) rather
//! than calling handlers directly, because the thing worth protecting is the
//! gate itself: an unauthenticated caller must never observe note content.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use eternalibre_notes::auth::{AuthError, AuthState};
use eternalibre_notes::routes::{self, AppState};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use tower::util::ServiceExt;

/// One shared notes directory for the whole test binary.
///
/// `ETERNALIBRE_NOTES_DIR` is process-global, so it must be set exactly once
/// before any test can race to change it.
fn shared_notes_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let mut p = std::env::temp_dir();
        p.push(format!("eternalibre-it-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        std::env::set_var("ETERNALIBRE_NOTES_DIR", &p);
        p
    })
}

fn temp_secret_path(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut p = std::env::temp_dir();
    p.push(format!(
        "eternalibre-it-{}-{}-{}",
        tag,
        std::process::id(),
        n
    ));
    p.set_extension("secret");
    let _ = std::fs::remove_file(&p);
    p
}

struct Fixture {
    app: axum::Router,
    auth: Arc<AuthState>,
    secret_file: PathBuf,
    token: Option<String>,
    secret: String,
    base_path: String,
}

impl Fixture {
    /// An app whose secret file does not exist yet.
    ///
    /// TOTP enrolment happens in the terminal before the listener binds, so
    /// this state is only reachable in-process. The router still handles it,
    /// which is what lets these tests assert the defensive 503 path.
    fn fresh(tag: &str) -> Self {
        Self::build(tag, "")
    }

    /// An app mounted under a reverse-proxy subdirectory.
    fn with_base_path(tag: &str, base_path: &str) -> Self {
        Self::build(tag, base_path)
    }

    fn build(tag: &str, base_path: &str) -> Self {
        let secret_file = temp_secret_path(tag);
        let auth = Arc::new(AuthState::with_secret_path(&secret_file));
        Self {
            // `routes::app`, not `routes::router`: the real binary mounts
            // static assets alongside the gated routes, and a test that skips
            // that assembly cannot see asset regressions.
            app: routes::app(AppState {
                auth: auth.clone(),
                base_path: base_path.into(),
            }),
            auth,
            secret_file,
            token: None,
            secret: String::new(),
            base_path: base_path.to_string(),
        }
    }

    /// Completes enrolment against the `AuthState` directly, which is what the
    /// terminal setup flow does before the server starts.
    fn provision(mut self) -> Self {
        let material = self.auth.begin_setup().unwrap();
        self.secret = material.secret;
        let code = valid_code(&self.secret);
        self.token = Some(self.auth.verify_setup(&code).expect("provisioning"));
        self
    }

    /// An app that has completed setup, with a live session cookie.
    fn provisioned(tag: &str) -> Self {
        Self::fresh(tag).provision()
    }

    /// Logs in over HTTP, exercising the real `/login` route.
    ///
    /// A rejected code just means the 30s TOTP window rolled over between
    /// enrolment and this call, so retry against the live secret rather than
    /// failing on a timing coincidence.
    async fn login_over_http(self) -> Self {
        // Enrol first if the caller hasn't, so this can be chained onto a
        // bare fixture.
        let mut f = if self.secret.is_empty() {
            self.provision()
        } else {
            self
        };
        for _ in 0..3 {
            let code = valid_code(&f.secret);
            let (status, _, cookie) = f.post_form("/login", &format!("code={code}")).await;
            if status == StatusCode::SEE_OTHER {
                f.token = Some(session_value(&cookie.expect("login sets a cookie")));
                return f;
            }
        }
        panic!("could not log in over HTTP with a valid code");
    }

    /// The URL the app is *expected* to emit for an app-absolute path.
    ///
    /// Requests themselves are always app-absolute: a reverse proxy strips
    /// the base prefix before forwarding, so the app never sees it. The base
    /// path only shows up in generated hrefs, form actions, and redirects.
    fn expect(&self, app_path: &str) -> String {
        format!("{}{}", self.base_path, app_path)
    }

    fn cookie_header(&self) -> String {
        format!("eternalibre_session={}", self.token.clone().unwrap())
    }

    async fn get(&self, uri: &str) -> (StatusCode, String) {
        let res = self
            .app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        finish(res).await
    }

    /// GET carrying the session cookie.
    async fn get_authed(&self, uri: &str) -> (StatusCode, String) {
        finish(self.get_authed_response(uri, None).await).await
    }

    async fn get_authed_with_theme(&self, uri: &str, theme: &str) -> axum::response::Response {
        self.get_authed_response(uri, Some(theme)).await
    }

    async fn get_authed_response(
        &self,
        uri: &str,
        theme: Option<&str>,
    ) -> axum::response::Response {
        let mut cookie = self.cookie_header();
        if let Some(theme) = theme {
            cookie.push_str(&format!("; theme={theme}"));
        }
        self.app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    /// POST as an anonymous visitor.
    async fn post_form(&self, uri: &str, body: &str) -> (StatusCode, String, Option<String>) {
        let res = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        respond(res).await
    }

    /// POST carrying the session cookie.
    async fn post_authed(&self, uri: &str, body: &str) -> (StatusCode, String, Option<String>) {
        let res = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::COOKIE, self.cookie_header())
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        respond(res).await
    }

    /// Creates a note and returns its id.
    async fn create_note(&self) -> String {
        let (_, _, loc) = self.post_authed("/notes", "").await;
        loc.expect("create redirects to the new note")
            .rsplit('/')
            .next()
            .unwrap()
            .to_string()
    }

    /// GET with an arbitrary cookie string, for spoofed-credential checks.
    async fn get_with_cookie(&self, cookie: &str) -> (StatusCode, String) {
        let res = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        finish(res).await
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.secret_file);
    }
}

async fn finish(res: axum::response::Response) -> (StatusCode, String) {
    let status = res.status();
    let loc = res
        .headers()
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| format!("LOCATION: {s}"));
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if let Some(l) = loc {
        text.push('\n');
        text.push_str(&l);
    }
    (status, text)
}

async fn respond(res: axum::response::Response) -> (StatusCode, String, Option<String>) {
    let status = res.status();
    let cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let loc = res
        .headers()
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    (status, text, cookie.or(loc))
}

fn session_value(set_cookie: &str) -> String {
    set_cookie
        .split(';')
        .next()
        .expect("cookie pair")
        .split_once('=')
        .expect("key=value")
        .1
        .to_string()
}

/// What a real authenticator app would display for this secret right now.
fn current_code(encoded_secret: &str) -> String {
    use totp_rs::{Algorithm, Builder, Secret};
    Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(6)
        .with_skew(1)
        .with_step_duration(30)
        .with_account_name("local")
        .with_issuer(Some("EternaLibre Notes"))
        .with_secret(Secret::try_from_base32(encoded_secret).unwrap())
        .build()
        .unwrap()
        .generate_current()
        .to_string()
}

/// Rejects a secret that is not a usable Base32 TOTP key, with a readable
/// message. A silently empty code would otherwise look like a TOTP mismatch.
fn valid_code(secret: &str) -> String {
    assert!(
        secret.len() >= 16 && !secret.is_empty(),
        "test fixture has an unusable secret: {secret:?}"
    );
    let code = current_code(secret);
    assert_eq!(
        code.len(),
        6,
        "expected a 6-digit code, got {code:?} from a bad secret"
    );
    code
}

// ---------------------------------------------------------------- gating ----

/// Every data-bearing route must refuse an anonymous caller. This is the core
/// security assertion: note content must not leak to an unauthenticated peer.
#[tokio::test]
async fn anonymous_cannot_read_any_note_route() {
    shared_notes_dir();
    let f = Fixture::provisioned("anon");

    for uri in ["/", "/notes/some-id", "/about", "/export", "/preview"] {
        let (status, body) = f.get(uri).await;
        assert!(
            status == StatusCode::SEE_OTHER || status == StatusCode::UNAUTHORIZED,
            "{uri} returned {status}, expected a redirect or 401"
        );
        assert!(
            !body.contains("app-body"),
            "{uri} leaked the app shell to an anonymous caller"
        );
    }
}

#[tokio::test]
async fn anonymous_always_redirects_to_login() {
    let f = Fixture::provisioned("anon-login");
    let (status, body) = f.get("/").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(body.contains("LOCATION: /login"), "got: {body}");
}

#[tokio::test]
async fn forged_session_cookie_is_rejected() {
    let f = Fixture::provisioned("forged");

    let res = f
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, "eternalibre_session=0000000000000000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(f.auth.session_count(), 1, "forged token created no session");
}

#[tokio::test]
async fn empty_session_cookie_is_rejected() {
    let f = Fixture::provisioned("emptycookie");
    let (status, _) = f.get_with_cookie("eternalibre_session=").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
}

// ------------------------------------------------------------ access with ----

#[tokio::test]
async fn authenticated_user_can_reach_the_app() {
    let f = Fixture::provisioned("ok");
    let (status, body) = f.get_authed("/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("app-body"), "expected the app shell");
    assert!(body.contains("EternaLibre Notes"));
}

#[tokio::test]
async fn note_content_is_invisible_until_authenticated() {
    let f = Fixture::provisioned("leak");
    let id = f.create_note().await;

    let marker = "TOPSECRET-CANARY-9f2a";
    let (status, _, _) = f
        .post_authed(
            &format!("/notes/{id}"),
            &format!("_method=put&title=Canary&content={marker}"),
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    // The authenticated user does see it, proving the marker exists at all.
    let (status, authed) = f.get_authed(&format!("/notes/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(authed.contains(marker), "test is not exercising real data");

    // An anonymous request for the same note must not echo it back.
    let (status, anon) = f.get(&format!("/notes/{id}")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(
        !anon.contains(marker),
        "note content leaked to an unauthenticated request"
    );
    assert!(!anon.contains("Canary"), "note title leaked");

    // Nor does the index page list it.
    let (_, index) = f.get("/").await;
    assert!(!index.contains("Canary"), "note title leaked via index");
}

#[tokio::test]
async fn preview_requires_authentication() {
    let f = Fixture::provisioned("preview-gate");

    let (status, body, _) = f.post_form("/preview", "content=%23+Hello").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(
        !body.contains("<h1>"),
        "markdown was rendered for an anonymous caller"
    );
}

#[tokio::test]
async fn preview_works_once_authenticated() {
    let f = Fixture::provisioned("preview-ok");
    let (status, body, _) = f.post_authed("/preview", "content=%23+Hello").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<h1>Hello</h1>"), "got: {body}");
}

#[tokio::test]
async fn export_requires_authentication() {
    let f = Fixture::provisioned("export-gate");
    let (status, body) = f.get("/export").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(!body.contains("EternaLibre Notes Export"));
}

#[tokio::test]
async fn single_note_export_menu_is_disabled_without_a_selected_note() {
    let f = Fixture::provisioned("note-export-menu");
    let (status, body) = f.get_authed("/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("aria-disabled=\"true\""));
    assert!(!body.contains("/export/pdf"));
    assert!(!body.contains("/export/markdown"));
    assert!(!body.contains("/export/html"));

    let id = f.create_note().await;
    let (status, selected) = f.get_authed(&format!("/notes/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(selected.contains("Print/PDF"));
    assert!(selected.contains("menu-submenu-list"));
    for format in ["pdf", "markdown", "html"] {
        assert!(
            selected.contains(&format!("href=\"/notes/{id}/export/{format}\"")),
            "selected note should expose the {format} export"
        );
    }
}

#[tokio::test]
async fn single_note_exports_require_authentication() {
    let f = Fixture::provisioned("note-export-gate");
    let id = f.create_note().await;
    for format in ["pdf", "markdown", "html"] {
        let (status, body) = f.get(&format!("/notes/{id}/export/{format}")).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert!(!body.contains("Export Secret"));
    }
}

#[tokio::test]
async fn markdown_export_downloads_the_saved_note() {
    let f = Fixture::provisioned("markdown-export");
    let id = f.create_note().await;
    let (status, _, _) = f
        .post_authed(
            &format!("/notes/{id}"),
            "_method=put&title=Saved+Version&content=First+paragraph",
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let response = f
        .get_authed_response(&format!("/notes/{id}/export/markdown"), None)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/markdown; charset=utf-8"
    );
    assert_eq!(
        response.headers()[header::CONTENT_DISPOSITION],
        "attachment; filename=\"saved-version.md\""
    );
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "private, no-store"
    );
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&body),
        "# Saved Version\n\nFirst paragraph"
    );
}

#[tokio::test]
async fn html_export_is_standalone_and_uses_the_selected_theme() {
    let f = Fixture::provisioned("html-export");
    let id = f.create_note().await;
    let (status, _, _) = f
        .post_authed(
            &format!("/notes/{id}"),
            "_method=put&title=HTML+Note&content=%23+Heading%0A%0ARendered+body",
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let response = f
        .get_authed_with_theme(&format!("/notes/{id}/export/html"), "Dracula")
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_DISPOSITION],
        "attachment; filename=\"html-note.html\""
    );
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let body = String::from_utf8_lossy(&body);
    assert!(body.contains("data-theme=\"Dracula\""));
    assert!(body.contains("--app-bg: #282a36;"));
    assert!(body.contains("<h1 class=\"note-export-title\">HTML Note</h1>"));
    assert!(body.contains("<h1>Heading</h1>"));
    assert!(body.contains("<p>Rendered body</p>"));
    assert!(
        body.contains(".markdown-body pre"),
        "application styles are inlined"
    );
}

#[tokio::test]
async fn pdf_export_opens_a_theme_aware_print_document() {
    let f = Fixture::provisioned("pdf-export");
    let id = f.create_note().await;
    let (status, _, _) = f
        .post_authed(
            &format!("/notes/{id}"),
            "_method=put&title=Print+Note&content=Printed+body",
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let response = f
        .get_authed_with_theme(&format!("/notes/{id}/export/pdf"), "Breeze Dark")
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/html; charset=utf-8"
    );
    assert_eq!(
        response.headers()[header::CONTENT_DISPOSITION],
        "inline; filename=\"print-note.html\""
    );
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let body = String::from_utf8_lossy(&body);
    assert!(body.contains("data-theme=\"Breeze Dark\""));
    assert!(body.contains("--app-bg: #232629;"));
    assert!(body.contains("print-color-adjust: exact;"));
    assert!(body.contains("window.print()"));
    assert!(body.contains("Printed body"));
}

#[tokio::test]
async fn exporting_a_missing_note_returns_not_found() {
    let f = Fixture::provisioned("missing-export");
    for format in ["pdf", "markdown", "html"] {
        let (status, _) = f
            .get_authed(&format!("/notes/does-not-exist/export/{format}"))
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn write_routes_require_authentication() {
    let f = Fixture::provisioned("write-gate");

    // Creating a note anonymously must not create anything.
    let before = std::fs::read_dir(shared_notes_dir()).unwrap().count();
    let (status, _, _) = f.post_form("/notes", "").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let after = std::fs::read_dir(shared_notes_dir()).unwrap().count();
    assert_eq!(before, after, "anonymous POST created a note");

    // Theme changes are gated too.
    let (status, _, _) = f.post_form("/theme", "theme=Dracula").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn deleting_a_note_requires_authentication() {
    let f = Fixture::provisioned("delete-gate");
    let id = f.create_note().await;

    let (status, _, _) = f.post_form(&format!("/notes/{id}"), "_method=delete").await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    // Still present, i.e. the anonymous delete did not take effect.
    let (status, _) = f.get_authed(&format!("/notes/{id}")).await;
    assert_eq!(status, StatusCode::OK);
}

// ------------------------------------------------------------------ auth ----

/// There is deliberately no `/setup` route. Enrolment happens in the
/// terminal before the listener binds, so an HTTP-reachable setup endpoint
/// could be claimed by anyone who reaches the port first.
#[tokio::test]
async fn there_is_no_web_setup_route() {
    let f = Fixture::fresh("no-setup-route");
    let (status, _) = f.get("/setup").await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "/setup must not exist over HTTP"
    );
}

/// A wrong code during terminal enrolment must leave no trace: no secret file
/// written and the device still unprovisioned.
#[tokio::test]
async fn wrong_enrolment_code_writes_nothing() {
    let f = Fixture::fresh("bad-enrol");
    f.auth.begin_setup().unwrap();

    assert_eq!(f.auth.verify_setup("000000"), Err(AuthError::InvalidCode));
    assert!(f.auth.needs_setup());
    assert!(
        !f.secret_file.exists(),
        "a failed enrolment must not write the secret"
    );
}

#[tokio::test]
async fn login_page_is_shown_after_provisioning() {
    let f = Fixture::provisioned("loginpage");
    let (status, body) = f.get("/login").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Unlock"));
    assert!(!body.contains("<svg"), "login must not show a QR code");
}

#[tokio::test]
async fn wrong_login_code_is_rejected_without_cookie() {
    let f = Fixture::provisioned("badlogin");
    let (status, body, cookie) = f.post_form("/login", "code=111111").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("was not accepted"));
    assert!(cookie.is_none());
}

#[tokio::test]
async fn valid_login_code_issues_a_hardened_session_cookie() {
    let f = Fixture::provisioned("goodlogin");
    let code = valid_code(&f.secret);
    let (status, _, cookie) = f.post_form("/login", &format!("code={code}")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let set_cookie = cookie.expect("login should set a session");
    assert!(set_cookie.contains("HttpOnly"), "cookie must be HttpOnly");
    assert!(set_cookie.contains("SameSite=Strict"), "got: {set_cookie}");
    assert!(
        set_cookie.contains("Max-Age=3600"),
        "expected a 1 hour session, got: {set_cookie}"
    );
    assert!(
        set_cookie.contains("Path=/;"),
        "at the root the cookie path should be the default path, got: {set_cookie}"
    );
}

/// Unprovisioned is only reachable in-process now, so the login page says so
/// instead of redirecting to a route that no longer exists.
#[tokio::test]
async fn login_reports_unprovisioned_device() {
    let f = Fixture::fresh("login-unprov");
    let (status, body) = f.get("/login").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body.contains("terminal"), "got: {body}");
    assert!(!body.contains("/setup"), "must not point at a dead route");
}

#[tokio::test]
async fn logout_invalidates_the_session() {
    let f = Fixture::provisioned("logout");
    let token = f.token.clone().unwrap();
    assert!(f.auth.validate_session(&token));

    let res = f
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/logout")
                .header(header::COOKIE, f.cookie_header())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);

    assert!(
        !f.auth.validate_session(&token),
        "server must forget the token, not just clear the cookie"
    );

    // The now-dead cookie no longer grants access.
    let res = f
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, format!("eternalibre_session={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn sessions_survive_across_requests() {
    let f = Fixture::provisioned("persist");
    for _ in 0..3 {
        let (status, _) = f.get_authed("/").await;
        assert_eq!(status, StatusCode::OK, "session should persist");
    }
    assert_eq!(f.auth.session_count(), 1);
}

#[tokio::test]
async fn separate_logins_get_separate_sessions() {
    let f = Fixture::provisioned("multi");
    let code = valid_code(&f.secret);
    let (_, _, cookie) = f.post_form("/login", &format!("code={code}")).await;
    let second = session_value(&cookie.unwrap());

    assert_ne!(second, f.token.clone().unwrap());
    assert_eq!(f.auth.session_count(), 2);
}

#[tokio::test]
async fn theme_cookie_alongside_session_still_works() {
    let f = Fixture::provisioned("themecookie");

    let res = f
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header(
                    header::COOKIE,
                    format!(
                        "theme=Dracula; eternalibre_session={}",
                        f.token.clone().unwrap()
                    ),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("data-theme=\"Dracula\""));
}

// ----------------------------------------------------------- about page ----

/// The about page's mobile styling hangs entirely off this one class.
///
/// `style.css` turns `.back-link` into a full-width 44px touch target on
/// mobile, matching the HIG minimum the rest of the app enforces. Drop the
/// class and the only way off the page silently reverts to a ~13px text
/// link — which is exactly the "assumes a desktop view" problem it was
/// added to fix. The heading rule is anchored on `main > h1` and so needs
/// no class, but the link does.
#[tokio::test]
async fn about_page_back_link_is_styled() {
    let f = Fixture::fresh("about-link").login_over_http().await;
    let (status, body) = f.get_authed("/about").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(r#"class="back-link""#),
        "about page is missing the back-link class that style.css targets"
    );
    assert!(
        body.contains("Back to notes"),
        "about page should still offer a way back"
    );
}

/// The about page is a separate document from the app shell, so it inlines
/// `style.css` itself. If that inlining were dropped, every mobile rule
/// would be dead on this page specifically.
#[tokio::test]
async fn about_page_inlines_the_stylesheet() {
    let f = Fixture::fresh("about-css").login_over_http().await;
    let (status, body) = f.get_authed("/about").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(".about-page main > h1"),
        "about page should inline the stylesheet that styles it"
    );
    // Match the full selector, not just the class name. A substring check on
    // `.back-link` also matches `.back-link-foo`, which is how this test
    // passed the first time it was run against a deliberately broken sheet.
    assert!(
        body.contains(".about-page .back-link {"),
        "the inlined sheet should carry the back-link rules"
    );
}

/// Every screen must declare a viewport, or mobile browsers fall back to a
/// ~980px layout viewport and scale the whole page down.
///
/// That silently disables every `@media (max-width: 600px)` rule in the
/// stylesheet, so a screen can carry a full set of mobile rules and still
/// render the desktop layout — which is exactly what happened to `/about`.
/// The failure is invisible in a desktop browser and invisible to any test
/// that only asserts the CSS was inlined, which is why it is pinned here
/// per-screen rather than once.
#[tokio::test]
async fn every_screen_declares_a_viewport() {
    let f = Fixture::fresh("viewport").login_over_http().await;
    for path in ["/", "/about"] {
        let (status, body) = f.get_authed(path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(
            body.contains(r#"name="viewport""#)
                && body.contains(r#"content="width=device-width, initial-scale=1"#),
            "{path} is missing a viewport meta tag; mobile will scale it down \
             and none of the max-width media queries will match"
        );
    }
}

/// `app.js` is inlined into the page, so it cannot call the server-side
/// `base.url()` helper. Its one `fetch()` has to build the URL itself, which
/// is what `window.__BASE_PATH__` is for.
///
/// Without it, `fetch('/preview')` resolves against the origin root. Behind
/// a reverse proxy mounted at a subdirectory that never reaches the app, and
/// the proxy's own 404 body gets swapped into the preview pane. The symptom
/// is a preview pane full of "404 Not Found" while typing.
#[tokio::test]
async fn the_base_path_is_handed_to_client_side_js() {
    // With a base path set, the browser must be told about it.
    let f = Fixture::with_base_path("base-js", "/forgejo")
        .login_over_http()
        .await;
    let (status, body) = f.get_authed("/").await;
    assert_eq!(status, StatusCode::OK);
    // Backtick template literal, matching the `__INITIAL_PREVIEW__` line
    // above it and escaped by the same `escape_js` helper.
    assert!(
        body.contains("window.__BASE_PATH__ = `/forgejo`;"),
        "client JS should be handed the base path"
    );

    // Without one, the value must be empty rather than the string "undefined",
    // so the fetch URL stays byte-identical to the pre-fix behaviour.
    let plain = Fixture::fresh("base-js-none").login_over_http().await;
    let (status, body) = plain.get_authed("/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("window.__BASE_PATH__ = ``;"),
        "with no base path the client value should be empty"
    );
}

/// The `fetch()` in `app.js` must actually use the injected value. This is
/// the assertion that would have caught the original bug: `app.js` is
/// inlined verbatim, so a regression there is invisible to every other test.
#[tokio::test]
async fn the_preview_fetch_uses_the_injected_base_path() {
    let f = Fixture::with_base_path("fetch-base", "/forgejo")
        .login_over_http()
        .await;
    let (status, body) = f.get_authed("/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("fetch(base + '/preview'"),
        "app.js should build the preview URL from __BASE_PATH__"
    );
    assert!(
        !body.contains("fetch('/preview'"),
        "app.js must not hardcode an origin-absolute /preview"
    );
}

/// The about page advertises a version, and it must be the one in
/// `Cargo.toml`.
///
/// The title bar used to carry a hand-written `"v0.1"` that had already
/// drifted from the crate's `0.1.0`. Both now read `CARGO_PKG_VERSION`, so
/// this pins that they agree with the manifest rather than with each other.
#[tokio::test]
async fn about_page_reports_the_manifest_version() {
    let f = Fixture::fresh("about-version").login_over_http().await;
    let (status, body) = f.get_authed("/about").await;
    assert_eq!(status, StatusCode::OK);

    let version = env!("CARGO_PKG_VERSION");
    assert!(
        body.contains(&format!("Version {version}")),
        "about page should report version {version}"
    );
    // The title bar must show the same string, not a separately maintained one.
    let (shell_status, shell) = f.get_authed("/").await;
    assert_eq!(shell_status, StatusCode::OK);
    assert!(
        shell.contains(&format!("EternaLibre Notes v{version}")),
        "title bar should show v{version} from the manifest"
    );
}

/// The theme counts are derived from the palette, not typed in. If someone
/// adds or removes a theme, this keeps the page honest without a manual edit.
#[tokio::test]
async fn about_page_theme_counts_come_from_the_palette() {
    use eternalibre_notes::themes::PREDEFINED_THEMES;

    let total = PREDEFINED_THEMES.len();
    let dark = PREDEFINED_THEMES.iter().filter(|t| t.is_dark()).count();
    let light = total - dark;

    let f = Fixture::fresh("about-themes").login_over_http().await;
    let (status, body) = f.get_authed("/about").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(&format!(
            "{total} built-in themes ({dark} dark, {light} light)"
        )),
        "about page should report {total} themes split {dark} dark / {light} light"
    );
}

/// Outbound links must carry `rel="noopener noreferrer"`. The app advertises
/// that it never talks to a third party, and these are the only links that
/// leave the machine, so they should not leak a referrer when followed.
#[tokio::test]
async fn about_page_external_links_are_hardened() {
    let f = Fixture::fresh("about-links").login_over_http().await;
    let (status, body) = f.get_authed("/about").await;
    assert_eq!(status, StatusCode::OK);

    for target in [
        "https://github.com/MARKMENTAL",
        "https://mentalnet.xyz/forgejo-v2/",
        "https://github.com/MARKMENTAL/tuxdock",
        "https://github.com/MARKMENTAL/mentalnet-gnu-linux",
    ] {
        assert!(body.contains(target), "about page should link to {target}");
    }

    // Check every external anchor, so a link added later without the
    // hardening attributes fails here instead of shipping quietly.
    let externals: Vec<&str> = body
        .match_indices("<a href=\"https://")
        .map(|(i, _)| &body[i..])
        .collect();
    assert!(
        !externals.is_empty(),
        "expected external links on the about page"
    );
    for a in externals {
        let tag = &a[..a.find('>').expect("unterminated anchor")];
        assert!(
            tag.contains(r#"rel="noopener noreferrer""#),
            "external link missing rel hardening: {tag}"
        );
    }
}

// ------------------------------------------------------------- base path ----

/// Every generated URL must carry the base prefix, or a browser behind the
/// proxy would resolve `/about` against the site root and 404.
#[tokio::test]
async fn base_path_prefixes_generated_links() {
    let f = Fixture::with_base_path("base-links", "/forgejo")
        .login_over_http()
        .await;
    let (status, body) = f.get_authed("/").await;
    assert_eq!(status, StatusCode::OK);

    // `/static/alpine.min.js` is the only externally-loaded asset;
    // `app.js` is inlined via `include_str!`, so it has no URL to rewrite.
    for target in [
        "/about",
        "/export",
        "/notes",
        "/theme",
        "/static/alpine.min.js",
    ] {
        let expected = f.expect(target);
        assert!(
            body.contains(&format!("\"{expected}\"")) || body.contains(&format!("'{expected}'")),
            "expected {expected} in generated markup"
        );
    }
    assert!(
        !body.contains("href=\"/about\""),
        "found a root-absolute link that would escape the prefix"
    );
    assert!(
        !body.contains("src=\"/static/"),
        "found a root-absolute asset URL that would escape the prefix"
    );
}

#[tokio::test]
async fn base_path_prefixes_single_note_export_links() {
    let f = Fixture::with_base_path("base-note-export", "/forgejo")
        .login_over_http()
        .await;
    let id = f.create_note().await;
    let (status, body) = f.get_authed(&format!("/notes/{id}")).await;
    assert_eq!(status, StatusCode::OK);

    for format in ["pdf", "markdown", "html"] {
        assert!(
            body.contains(&format!("href=\"/forgejo/notes/{id}/export/{format}\"")),
            "{format} export link should include the base path"
        );
    }
}

#[tokio::test]
async fn base_path_prefixes_redirects() {
    let f = Fixture::with_base_path("base-redirect", "/forgejo")
        .login_over_http()
        .await;

    // Creating a note redirects to the new note's URL.
    let (_, _, loc) = f.post_authed("/notes", "").await;
    let loc = loc.expect("create redirects");
    assert!(
        loc.starts_with("/forgejo/notes/"),
        "redirect escaped the base path: {loc}"
    );

    // An unauthenticated request must bounce within the prefix too.
    let anon = Fixture::with_base_path("base-anon", "/forgejo");
    let (status, body) = anon.get("/").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(
        body.contains("LOCATION: /forgejo/login"),
        "login redirect escaped the base path: {body}"
    );
}

#[tokio::test]
async fn base_path_scopes_the_session_cookie() {
    let f = Fixture::with_base_path("base-cookie", "/forgejo")
        .login_over_http()
        .await;
    // The fixture captured only the token value, so log in again to inspect
    // the raw Set-Cookie header.
    let code = valid_code(&f.secret);
    let (_, _, set_cookie) = f.post_form("/login", &format!("code={code}")).await;
    let raw = set_cookie.expect("login sets a cookie");
    assert!(
        raw.contains("Path=/forgejo"),
        "cookie must be scoped to the prefix, got: {raw}"
    );
}

#[tokio::test]
async fn base_path_login_form_posts_to_the_prefix() {
    let f = Fixture::with_base_path("base-form", "/forgejo").provision();
    let (status, body) = f.get("/login").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(r#"action="/forgejo/login""#),
        "login form action escaped the prefix"
    );
}

/// A reverse proxy strips the base path before forwarding, so the path the
/// app actually receives for a static asset is app-absolute.
///
/// This is the assertion that catches the bug where the static nest was
/// registered at `{base}/static`: the asset 404'd, `x-data="app()"` never
/// initialised, and every Alpine directive — including the File menu
/// dropdowns — silently stopped working.
///
/// It drives `routes::app`, the same assembly `main.rs` uses, rather than
/// hand-rolling a router, so it fails if the real configuration regresses.
#[tokio::test]
async fn alpine_is_served_app_absolute_even_with_a_base_path() {
    use eternalibre_notes::routes::{self, AppState};

    let secret_file = temp_secret_path("static-abs");
    let auth = Arc::new(AuthState::with_secret_path(&secret_file));
    let app = routes::app(AppState {
        auth,
        base_path: "/forgejo".into(),
    });

    let (status, body) = fetch(&app, "/static/alpine.min.js").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "Alpine must be served at the app-absolute path; a miss here leaves \
         x-data=\"app()\" uninitialised and the menus dead"
    );
    assert!(
        body.contains("Alpine"),
        "expected the Alpine bundle, got {} bytes",
        body.len()
    );

    // The prefixed path is not a route on the app: the proxy is what strips
    // the prefix, so the app must not also accept it.
    assert_ne!(
        fetch(&app, "/forgejo/static/alpine.min.js").await.0,
        StatusCode::OK,
        "static should not be reachable at the prefixed path"
    );

    let _ = std::fs::remove_file(&secret_file);
}

/// Alpine must be reachable *without* a session: it is what makes every
/// Alpine directive work, and it sits outside the auth gate.
#[tokio::test]
async fn alpine_is_served_without_a_session() {
    let f = Fixture::fresh("static-anon");
    let (status, body) = f.get("/static/alpine.min.js").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Alpine"));
}

/// The bytes are baked in at compile time, so the response can be cached
/// indefinitely and is unaffected by the working directory.
#[tokio::test]
async fn alpine_response_is_immutably_cacheable() {
    let f = Fixture::fresh("static-cache");
    let res = f
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/static/alpine.min.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let cache = res
        .headers()
        .get(header::CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(
        cache.contains("immutable"),
        "expected an immutable cache header, got {cache:?}"
    );
    let ctype = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(
        ctype.contains("javascript"),
        "expected a JavaScript content type, got {ctype:?}"
    );
}

/// The app must not depend on a `static/` directory existing at runtime.
/// This is the regression that bit a real deployment: the binary was moved
/// without its `static/` folder and every Alpine directive went dead.
#[tokio::test]
async fn alpine_is_available_without_a_static_directory() {
    use eternalibre_notes::routes::{self, AppState};

    let secret_file = temp_secret_path("static-nodir");
    let auth = Arc::new(AuthState::with_secret_path(&secret_file));
    let app = routes::app(AppState {
        auth,
        base_path: "".into(),
    });

    // Nothing is read from disk at request time; the bundle is linked in.
    let (status, body) = fetch(&app, "/static/alpine.min.js").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.len() > 10_000,
        "bundle looks truncated: {} bytes",
        body.len()
    );

    let _ = std::fs::remove_file(&secret_file);
}

/// GET a path and return its status and body.
async fn fetch(app: &axum::Router, path: &str) -> (StatusCode, String) {
    let res = app
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 4 << 20)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}
