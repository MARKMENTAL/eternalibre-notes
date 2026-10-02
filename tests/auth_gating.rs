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

// ---------------------------------------------------------------- folders ----

#[tokio::test]
async fn folder_creation_creates_a_note_in_the_folder() {
    let f = Fixture::provisioned("folder-create");
    let (status, _, loc) = f.post_authed("/folders", "name=Project+Notes").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let loc = loc.expect("folder creation redirects");
    assert!(loc.starts_with("/notes/"), "got: {loc}");

    // The new note should appear in the folder view.
    let (status, body) = f.get_authed("/?folder=Project%20Notes").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("Project Notes"),
        "folder nav should list the folder"
    );
    assert!(
        body.contains("Untitled"),
        "the new note should be in the folder"
    );
}

#[tokio::test]
async fn folder_filtering_shows_only_folder_notes() {
    let f = Fixture::provisioned("folder-filter");
    let root_id = f.create_note().await;
    let (_, _, loc) = f.post_authed("/folders", "name=Work").await;
    let work_loc = loc.expect("folder creation redirects");
    let work_id = work_loc.rsplit('/').next().unwrap().to_string();

    // Root view shows both.
    let (status, body) = f.get_authed("/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(&root_id),
        "root note should appear in All Notes"
    );
    assert!(
        body.contains(&work_id),
        "folder note should appear in All Notes"
    );

    // Folder view shows only the folder note.
    let (status, body) = f.get_authed("/?folder=Work").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains(&root_id),
        "root note should not appear in folder view"
    );
    assert!(
        body.contains(&work_id),
        "folder note should appear in folder view"
    );
}

#[tokio::test]
async fn new_note_in_folder_creates_with_prefix() {
    let f = Fixture::provisioned("folder-new-note");
    let (status, _, loc) = f.post_authed("/notes", "folder=Personal").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let loc = loc.expect("create redirects");
    let id = loc.rsplit('/').next().unwrap().to_string();

    // The file on disk should have the [Personal] prefix.
    let dir = shared_notes_dir();
    let entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    let found = entries
        .iter()
        .any(|name| name.starts_with("[Personal]") && name.contains(&id));
    assert!(
        found,
        "expected a [Personal]-prefixed file for {id}, got: {entries:?}"
    );
}

#[tokio::test]
async fn folder_route_requires_authentication() {
    let f = Fixture::provisioned("folder-gate");
    let (status, _, _) = f.post_form("/folders", "name=Secret").await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    // No note should have been created.
    let (status, body) = f.get("/").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(!body.contains("Secret"));
}

#[tokio::test]
async fn invalid_folder_name_is_rejected() {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

    let f = Fixture::provisioned("folder-invalid");
    for name in [
        "",
        "   ",
        "has]bracket",
        "has/slash",
        "has\\backslash",
        "..",
        ".hidden",
        "foo;bar",
        "foo|bar",
        "foo&bar",
        "foo$bar",
        "foo`bar",
        "foo(bar)",
        "foo<bar>",
    ] {
        let encoded = utf8_percent_encode(name, NON_ALPHANUMERIC).to_string();
        let body = format!("name={encoded}");
        let (status, _, loc) = f.post_authed("/folders", &body).await;
        // Should redirect back to / with an error, not to a new note.
        assert_eq!(
            status,
            StatusCode::SEE_OTHER,
            "folder name {name:?} should redirect back"
        );
        let loc = loc.expect("should have a Location header");
        assert!(
            loc.starts_with("/?error="),
            "expected redirect to /?error=..., got: {loc}"
        );
    }
}

#[tokio::test]
async fn folder_creation_error_shows_flash_message() {
    let f = Fixture::provisioned("folder-flash");
    let (_, _, loc) = f.post_authed("/folders", "name=has%2Fslash").await;
    let loc = loc.expect("should redirect");
    assert!(loc.starts_with("/?error="), "got: {loc}");

    // Follow the redirect and verify the flash message appears.
    let (status, body) = f
        .get_authed("/?error=folder+name+cannot+contain+%27%2F%27+or+%27%5C%27")
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("flash-message"),
        "page should contain the flash-message element"
    );
    assert!(
        body.contains("folder name cannot contain"),
        "flash message should show the validation error"
    );
}

// ------------------------------------------------------- moving a note ----

/// Every on-disk note file whose `[Folder]uuid` stem parses to `id`.
fn files_for_id(id: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(shared_notes_dir()) else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| {
            let stem = name.strip_suffix(".md").unwrap_or(name);
            let stripped = stem.strip_prefix('[').and_then(|s| s.split_once(']'));
            let parsed = stripped.map(|(_, rest)| rest).unwrap_or(stem);
            parsed == id
        })
        .collect();
    found.sort();
    found
}

#[tokio::test]
async fn move_route_renames_the_note_file() {
    let f = Fixture::provisioned("move-rename");
    let id = f.create_note().await;
    f.post_authed(&format!("/notes/{id}"), "title=Movable&content=Body")
        .await;

    let (status, _, loc) = f
        .post_authed(&format!("/notes/{id}/move"), "folder=Work")
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(loc.as_deref(), Some(format!("/notes/{id}").as_str()));

    // The file was renamed, not duplicated: the bare name is gone and exactly
    // one prefixed file claims the id.
    assert_eq!(files_for_id(&id), vec![format!("[Work]{id}.md")]);

    // Content survived the move.
    let (status, body) = f.get_authed(&format!("/notes/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Movable"), "title should survive the move");
    assert!(body.contains("Body"), "content should survive the move");
}

#[tokio::test]
async fn move_route_to_root_strips_the_prefix() {
    let f = Fixture::provisioned("move-root");
    let (_, _, loc) = f.post_authed("/folders", "name=Work").await;
    let work_id = loc
        .expect("redirect")
        .rsplit('/')
        .next()
        .unwrap()
        .to_string();
    assert_eq!(files_for_id(&work_id), vec![format!("[Work]{work_id}.md")]);

    // An empty folder is the root, which is how "All Notes" and the move
    // form's Unfiled option both express "no folder".
    let (status, _, _) = f
        .post_authed(&format!("/notes/{work_id}/move"), "folder=")
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(files_for_id(&work_id), vec![format!("{work_id}.md")]);
}

/// The bug this whole feature hangs on.
///
/// `save_note` derives the filename from `note.folder` but never removes the
/// file it replaced, so a "move" implemented as a re-save leaves the original
/// behind. `list_notes` then returns the same id twice and the sidebar renders
/// the note twice, with the two copies ordered by whatever `updated_at` each
/// file happened to carry. This pins the rename rather than the re-save.
#[tokio::test]
async fn move_route_leaves_no_duplicate_file_or_listing() {
    let f = Fixture::provisioned("move-nodup");
    let id = f.create_note().await;

    f.post_authed(&format!("/notes/{id}/move"), "folder=Work")
        .await;
    assert_eq!(
        files_for_id(&id).len(),
        1,
        "one file on disk should claim the id, got: {:?}",
        files_for_id(&id)
    );

    f.post_authed(&format!("/notes/{id}/move"), "folder=Home")
        .await;
    assert_eq!(
        files_for_id(&id).len(),
        1,
        "repeated moves must not accumulate files, got: {:?}",
        files_for_id(&id)
    );

    let (_, body) = f.get_authed("/").await;
    assert_eq!(
        body.matches(&format!("data-note-id=\"{id}\"")).count(),
        1,
        "the sidebar should list the note exactly once"
    );

    // The folder listing shows one copy, the root listing none.
    let (_, home) = f.get_authed("/?folder=Home").await;
    assert_eq!(home.matches(&format!("data-note-id=\"{id}\"")).count(), 1);
    let (_, work) = f.get_authed("/?folder=Work").await;
    assert!(
        !work.contains(&id),
        "the note left Work, so it must not appear there"
    );
}

#[tokio::test]
async fn move_route_rejects_invalid_folder_names_without_touching_the_file() {
    let f = Fixture::provisioned("move-invalid");
    let id = f.create_note().await;

    for (name, body) in [
        ("has]bracket", "folder=has%5Dbracket"),
        ("has/slash", "folder=has%2Fslash"),
        ("path traversal", "folder=..%2F..%2Fetc"),
        ("empty after trim", "folder=%20%20"),
    ] {
        let (status, _, loc) = f.post_authed(&format!("/notes/{id}/move"), body).await;
        assert_eq!(
            status,
            StatusCode::SEE_OTHER,
            "{name} should redirect back with an error"
        );
        assert!(
            loc.expect("redirect").starts_with("/?error="),
            "{name} should land on /?error="
        );
        assert_eq!(
            files_for_id(&id),
            vec![format!("{id}.md")],
            "{name} must leave the file where it was"
        );
    }
}

#[tokio::test]
async fn move_route_requires_authentication() {
    let f = Fixture::provisioned("move-gate");
    let id = f.create_note().await;

    let (status, _, loc) = f
        .post_form(&format!("/notes/{id}/move"), "folder=Secret")
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(loc.as_deref(), Some("/login"));

    assert_eq!(
        files_for_id(&id),
        vec![format!("{id}.md")],
        "an unauthenticated move must not reach the filesystem"
    );
}

#[tokio::test]
async fn move_route_redirects_through_the_base_path() {
    let f = Fixture::with_base_path("move-base", "/forgejo")
        .login_over_http()
        .await;
    let id = f.create_note().await;

    let (status, _, loc) = f
        .post_authed(&format!("/notes/{id}/move"), "folder=Work")
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        loc.as_deref(),
        Some(format!("/forgejo/notes/{id}").as_str())
    );
    assert_eq!(files_for_id(&id), vec![format!("[Work]{id}.md")]);
}

/// The drag affordances are server-rendered, so the hooks are assertable even
/// though the drag itself needs JavaScript.
#[tokio::test]
async fn sidebar_note_items_and_folder_items_carry_drag_hooks() {
    let f = Fixture::provisioned("drag-hooks");
    let id = f.create_note().await;
    f.post_authed(&format!("/notes/{id}/move"), "folder=Work")
        .await;

    let (status, body) = f.get_authed("/").await;
    assert_eq!(status, StatusCode::OK);

    assert!(
        body.contains(&format!("data-note-id=\"{id}\"")),
        "note items must expose their id for the drag payload"
    );
    assert!(
        body.contains("draggable=\"true\""),
        "note items must be draggable"
    );
    // The anchor must opt out, or the browser starts a native link drag and
    // the folder never lights up as a drop target.
    assert!(
        body.contains("draggable=\"false\""),
        "the inner anchor must opt out of native link dragging"
    );
    assert!(
        body.contains("data-drop-folder=\"Work\""),
        "each folder nav item must name its folder"
    );
    assert!(
        body.contains("data-drop-folder=\"\""),
        "All Notes must be a drop target that means the root"
    );
}

/// Desktop re-files through File > Move to Folder…. Each entry is its own form
/// posting to `/notes/:id/move`, so a click is a real POST and no JavaScript
/// performs the move.
#[tokio::test]
async fn the_move_menu_posts_the_move_route() {
    let f = Fixture::provisioned("move-menu");
    f.post_authed("/folders", "name=Alpha").await;
    f.post_authed("/folders", "name=Beta").await;
    let id = f.create_note().await;

    let (status, body) = f.get_authed(&format!("/notes/{id}")).await;
    assert_eq!(status, StatusCode::OK);

    assert!(
        body.contains("Move to Folder"),
        "the File menu should offer a Move to Folder submenu"
    );

    // Scope to the submenu before counting. `list_folders()` reads the shared
    // notes directory, which every test in this binary writes to, so the app's
    // folder list legitimately holds folders created by unrelated tests. A
    // whole-page count would therefore be asserting on global state.
    //
    // Anchored on the Alpine binding, not on the label text: `style.css` is
    // inlined into the page and its comments mention the menu by name, so a
    // text search finds the stylesheet first.
    let trigger = body
        .find("toggleSubmenu('noteMove'")
        .expect("the submenu trigger");
    let submenu_end = body[trigger..]
        .find("</ul>")
        .map(|i| trigger + i)
        .expect("the submenu list should close");
    let submenu = &body[trigger..submenu_end];

    let action = format!("action=\"/notes/{id}/move\"");

    // One destination per folder in the sidebar, plus (Unfiled). Counted from
    // the sidebar rather than hardcoded, because the shared notes directory
    // means other tests' folders are legitimately present. The invariant worth
    // pinning is that the menu and the sidebar agree on which folders exist.
    let nav = folder_nav_region(&body);
    let nav_items = nav.matches("data-drop-folder=\"").count();
    assert_eq!(
        submenu.matches(&action).count(),
        nav_items,
        "the submenu should offer (Unfiled) plus one entry per sidebar \
         folder ({nav_items} destinations), got:\n{submenu}"
    );

    // The two folders this test created must both be offered.
    assert!(
        submenu.contains("value=\"Alpha\""),
        "Alpha should be listed"
    );
    assert!(submenu.contains("value=\"Beta\""), "Beta should be listed");

    // Folder names run to 100 characters and the flyout is capped to the
    // viewport, so a long name wraps across lines. The untruncated name stays
    // reachable via the title attribute.
    assert!(
        submenu.contains("title=\"Alpha\""),
        "each folder button should carry its full name as a title"
    );

    // Un-filing comes first, matching the sidebar's own ordering.
    let unfiled = submenu
        .find("value=\"\"")
        .expect("the submenu should offer (Unfiled)");
    let alpha = submenu.find("value=\"Alpha\"").expect("Alpha");
    assert!(unfiled < alpha, "(Unfiled) should be listed first");

    // (Unfiled) posts an empty folder, which the route reads as the root.
    assert!(
        body.contains("<input type=\"hidden\" name=\"folder\" value=\"\">"),
        "the submenu should offer a way to un-file a note"
    );
    assert!(
        body.contains("<input type=\"hidden\" name=\"folder\" value=\"Alpha\">"),
        "each folder should post its own name"
    );

    // Disabled with no note open, matching Export Note As….
    let (status, empty) = f.get_authed("/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        empty.contains("Move to Folder"),
        "the trigger should still render without a note"
    );
}

/// Mobile's re-filing control. It must stay in the markup at every breakpoint —
/// CSS decides which entry point shows — and it must work without JavaScript,
/// which is the whole reason it exists beside the menu.
#[tokio::test]
async fn the_move_bar_survives_for_mobile() {
    let f = Fixture::provisioned("move-bar");
    f.post_authed("/folders", "name=Alpha").await;
    let id = f.create_note().await;

    let (status, body) = f.get_authed(&format!("/notes/{id}")).await;
    assert_eq!(status, StatusCode::OK);

    assert!(
        body.contains("class=\"move-folder-bar\""),
        "the mobile bar should be in the markup"
    );
    assert!(
        body.contains(&format!("action=\"/notes/{id}/move\"")),
        "the bar should post to the move route"
    );
    assert!(
        body.contains("name=\"folder\""),
        "the bar needs a folder field"
    );

    // A select cannot submit a form by itself. If an auto-submit handler came
    // back, the button would look redundant and the control would silently stop
    // working with JS off — which is the only reason it is here.
    //
    // Both bounds are anchored on markup. `style.css` is inlined into the page,
    // so a search for `move-folder-bar` finds the stylesheet first and the
    // slice covers CSS, where both assertions pass without inspecting
    // anything. The bar holds exactly one form, so that form's close tag is a
    // reliable end bound covering both its attributes and its button.
    let bar_start = body
        .find("class=\"move-folder-bar\"")
        .expect("the bar markup");
    let bar_end = body[bar_start..]
        .find("</form>")
        .map(|i| bar_start + i + "</form>".len())
        .expect("the bar holds one form");
    let bar = &body[bar_start..bar_end];

    assert!(
        !bar.contains("x-on:change"),
        "the bar must not auto-submit on change; the button is the only \
         submit path and the control has to work without JS"
    );
    assert!(
        bar.contains("<button type=\"submit\""),
        "the bar needs its submit button"
    );
}

/// Neither re-filing entry point may carry `desktop-only`.
///
/// This is a regression guard for a bug that shipped: the bar had
/// `desktop-only` *and* a `@media (min-width: 601px) { display: none }` rule.
/// `.desktop-only` hides at `max-width: 600px`, so the two rules composed to
/// hide the bar at every width, leaving phones with no way to re-file a note
/// at all — the menu was `desktop-only` too, and HTML5 drag does not fire on
/// touch.
///
/// Nothing in a Rust test can observe `display: none`, which is why the earlier
/// version of this check passed while the control was invisible. Pinning the
/// *contract* — the class must be absent from both, with the bar hidden by a
/// `min-width` query instead — is what actually catches the mistake.
#[tokio::test]
async fn move_entry_points_are_not_desktop_only() {
    let f = Fixture::provisioned("move-not-desktop-only");
    f.post_authed("/folders", "name=Alpha").await;
    let id = f.create_note().await;

    let (status, body) = f.get_authed(&format!("/notes/{id}")).await;
    assert_eq!(status, StatusCode::OK);

    assert!(
        !body.contains("class=\"move-folder-bar desktop-only\""),
        "`.desktop-only` hides at max-width 600px, so adding it to the bar \
         would hide the mobile control that only exists below that width"
    );

    let trigger = body
        .find("toggleSubmenu('noteMove'")
        .expect("the submenu trigger");
    let submenu_end = body[trigger..]
        .find("</ul>")
        .map(|i| trigger + i)
        .expect("the submenu list should close");
    let submenu = &body[trigger..submenu_end];

    assert!(
        !submenu.contains("desktop-only"),
        "the File submenu is the mobile entry point too and must stay visible \
         at every width"
    );
}

/// The move control must not be swallowed by the browser as an illegally
/// nested form: a form inside a form is invalid and the inner element is
/// discarded outright.
#[tokio::test]
async fn the_move_form_is_a_sibling_of_the_note_form() {
    let f = Fixture::provisioned("move-form-shape");
    let id = f.create_note().await;
    let (status, body) = f.get_authed(&format!("/notes/{id}")).await;
    assert_eq!(status, StatusCode::OK);

    let note_form = body
        .find("<form id=\"note-form\"")
        .expect("the note form should be present");
    let note_form_end = body[note_form..]
        .find("</form>")
        .map(|i| note_form + i)
        .expect("the note form should be closed");
    // Match the markup, not the bare class name: `style.css` is inlined into
    // every page, so a plain `find("move-folder-form")` lands on the
    // stylesheet text and passes while the real control is nested wrongly.
    let move_form = body
        .find("class=\"move-folder-form\"")
        .expect("the move form should be present");

    assert!(
        move_form > note_form_end,
        "a form nested inside #note-form is invalid HTML and browsers drop the \
         inner element entirely, silently deleting the no-JS move control"
    );

    assert!(
        body.contains(&format!("action=\"/notes/{id}/move\"")),
        "the move form should post to the move route"
    );
    assert!(
        body.contains("name=\"folder\""),
        "the move form needs a folder field"
    );
}

/// `app.js` is inlined verbatim, so a regression in it is invisible to every
/// other test in the suite. Behind a base path an origin-absolute move URL
/// resolves against the site root, never reaches the app, and the proxy's own
/// 404 comes back.
#[tokio::test]
async fn the_move_fetch_uses_the_injected_base_path() {
    let f = Fixture::with_base_path("move-fetch-base", "/forgejo")
        .login_over_http()
        .await;
    let (status, body) = f.get_authed("/").await;
    assert_eq!(status, StatusCode::OK);

    assert!(
        body.contains("fetch(base + '/notes/'"),
        "app.js should build the move URL from __BASE_PATH__"
    );
    assert!(
        !body.contains("fetch('/notes/'"),
        "app.js must not hardcode an origin-absolute move path"
    );
}

#[tokio::test]
async fn sidebar_shows_folder_navigation() {
    let f = Fixture::provisioned("folder-nav");
    f.post_authed("/folders", "name=Alpha").await;
    f.post_authed("/folders", "name=Beta").await;

    let (status, body) = f.get_authed("/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("All Notes"), "should show All Notes");
    assert!(body.contains("Alpha"), "should show Alpha folder");
    assert!(body.contains("Beta"), "should show Beta folder");
    assert!(body.contains("nav-count"), "should show note counts");
}

/// The `folder-nav` list, and nothing else.
///
/// Scoped deliberately: `.nav-count` also appears in the inlined stylesheet,
/// and counting the whole body would let a badge elsewhere in the page stand in
/// for a nav badge that is not there — which is how the test below would come
/// to pass while showing the wrong thing.
fn folder_nav_region(body: &str) -> &str {
    let start = body
        .find("<ul class=\"folder-nav\">")
        .expect("the folder nav should be present");
    let rest = &body[start..];
    let end = rest.find("</ul>").expect("the folder nav should be closed");
    &rest[..end]
}

/// Counts the badges inside one nav item's link.
///
/// Returns `None` when the label is not in the nav at all, so a test can tell
/// "no badge" apart from "no such folder" instead of both reading as zero.
fn nav_count_for(region: &str, label: &str) -> Option<usize> {
    let anchor = region
        .split("<a href=")
        .find(|chunk| chunk.contains(&format!(">{label}</span>")))
        .unwrap_or_else(|| panic!("no nav item labelled {label:?} in:\n{region}"));
    Some(anchor.matches("nav-count").count())
}

#[tokio::test]
async fn all_notes_view_shows_every_folder_count() {
    let f = Fixture::provisioned("nav-count-all");
    f.post_authed("/folders", "name=Alpha").await;
    f.post_authed("/folders", "name=Beta").await;

    let (status, body) = f.get_authed("/").await;
    assert_eq!(status, StatusCode::OK);
    let nav = folder_nav_region(&body);

    // Every folder is visible at once here, so the counts are a real summary
    // and every one of them earns its place.
    assert_eq!(
        nav_count_for(nav, "All Notes"),
        Some(1),
        "All Notes should be badged in the All Notes view"
    );
    assert_eq!(
        nav_count_for(nav, "Alpha"),
        Some(1),
        "each folder should be badged in the All Notes view"
    );
    assert_eq!(nav_count_for(nav, "Beta"), Some(1));
}

#[tokio::test]
async fn folder_view_shows_only_the_active_folder_count() {
    let f = Fixture::provisioned("nav-count-one");
    f.post_authed("/folders", "name=Alpha").await;
    f.post_authed("/folders", "name=Beta").await;

    let (status, body) = f.get_authed("/?folder=Alpha").await;
    assert_eq!(status, StatusCode::OK);
    let nav = folder_nav_region(&body);

    // Exactly one badge: the folder you are in. The siblings are not merely
    // un-highlighted, they carry nothing at all.
    assert_eq!(
        nav_count_for(nav, "Alpha"),
        Some(1),
        "the open folder should be badged"
    );
    assert_eq!(
        nav_count_for(nav, "Beta"),
        Some(0),
        "a sibling folder should carry no badge, not a zero"
    );
    assert_eq!(
        nav_count_for(nav, "All Notes"),
        Some(0),
        "a badge there would be this folder's count under a different label"
    );

    // Belt and braces: the whole nav holds one badge, so a badge appearing
    // anywhere unexpected fails rather than being absorbed above.
    assert_eq!(
        nav.matches("nav-count").count(),
        1,
        "a folder view should show exactly one badge, got:\n{nav}"
    );
}

#[tokio::test]
async fn note_in_folder_shows_folder_in_editor() {
    let f = Fixture::provisioned("folder-editor");
    let (_, _, loc) = f.post_authed("/folders", "name=Projects").await;
    let loc = loc.expect("folder creation redirects");
    let id = loc.rsplit('/').next().unwrap().to_string();

    let (status, body) = f.get_authed(&format!("/notes/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("Projects"),
        "editor should show the note's folder"
    );
    assert!(
        body.contains("note-folder"),
        "should use the note-folder class"
    );
}

#[tokio::test]
async fn folder_creation_with_special_characters() {
    let f = Fixture::provisioned("folder-special");
    // Spaces and unicode are fine.
    let (status, _, loc) = f
        .post_authed("/folders", "name=My+%E2%9C%93+Projects")
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let loc = loc.expect("folder creation redirects");
    assert!(loc.starts_with("/notes/"));
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
