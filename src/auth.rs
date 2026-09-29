use qrcode::render::svg;
use qrcode::QrCode;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{Duration, Instant};
use totp_rs::{Algorithm, Builder, Secret, Totp};

/// File holding the Base32 TOTP secret. Deleting it re-runs setup, which is
/// the documented recovery path for a lost authenticator.
const SECRET_FILE: &str = ".totp_secret";

/// Sessions are intentionally short: this app may be reachable from the
/// network, so a one-hour lifetime bounds the value of a stolen cookie.
const SESSION_TTL: Duration = Duration::from_secs(60 * 60);

pub const SESSION_COOKIE: &str = "eternalibre_session";

/// Issuer label shown in the authenticator app.
const ISSUER: &str = "EternaLibre Notes";
const ACCOUNT: &str = "local";

const DIGITS: u8 = 6;
const PERIOD: u64 = 30;
/// Accept the neighbouring time step too, so ~30s of clock drift either way
/// still validates.
const SKEW: u16 = 1;

struct Inner {
    /// Committed secret, loaded from disk at startup.
    secret: Option<String>,
    /// Secret shown in the QR code but not yet confirmed by the user. Only
    /// written to disk once a valid code proves the user can read it.
    pending_secret: Option<String>,
    sessions: HashMap<String, Instant>,
}

pub struct AuthState {
    inner: RwLock<Inner>,
    path: PathBuf,
}

impl Default for AuthState {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthState {
    pub fn new() -> Self {
        Self::with_secret_path(SECRET_FILE)
    }

    /// `path` is injectable so tests never touch the real `.totp_secret`.
    pub fn with_secret_path(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let secret = std::fs::read_to_string(&path)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        Self {
            inner: RwLock::new(Inner {
                secret,
                pending_secret: None,
                sessions: HashMap::new(),
            }),
            path,
        }
    }

    /// True until a TOTP secret has been committed to disk.
    ///
    /// The server consults this once at startup to decide whether to run the
    /// terminal enrolment flow before binding the listener.
    pub fn needs_setup(&self) -> bool {
        self.inner.read().unwrap().secret.is_none()
    }

    /// Generates a fresh secret and returns everything the terminal enrolment
    /// flow needs: an inline SVG QR code, the plaintext secret for manual
    /// entry, and the `otpauth://` URI.
    ///
    /// Regenerating replaces `pending_secret` and invalidates any previously
    /// displayed QR code, so the code on screen is always the one that works.
    pub fn begin_setup(&self) -> Result<SetupMaterial, String> {
        let totp = build_totp(Secret::generate()).map_err(|e| e.to_string())?;
        let encoded = totp.secret().to_base32();

        let uri = totp.to_url().map_err(|e| e.to_string())?;
        let code = QrCode::new(uri.as_bytes()).map_err(|e| e.to_string())?;
        let qr_svg = code
            .render::<svg::Color>()
            .min_dimensions(240, 240)
            .quiet_zone(true)
            .build();

        self.inner.write().unwrap().pending_secret = Some(encoded.clone());

        Ok(SetupMaterial {
            secret: encoded,
            qr_svg,
            qr_uri: uri,
        })
    }

    /// Verifies a code against the pending secret. On success the secret is
    /// committed to disk and a session token is returned.
    pub fn verify_setup(&self, code: &str) -> Result<String, AuthError> {
        let mut guard = self.inner.write().unwrap();
        let pending = guard
            .pending_secret
            .clone()
            .ok_or(AuthError::NoPendingSecret)?;

        if !verify_code(&pending, code) {
            return Err(AuthError::InvalidCode);
        }

        // Only persist once the user has proven they can read the code, so a
        // half-finished setup never locks them out.
        write_secret_file(&self.path, &pending).map_err(|_| AuthError::PersistFailed)?;

        guard.secret = Some(pending);
        guard.pending_secret = None;
        Ok(new_session_token(&mut guard))
    }

    pub fn verify_login(&self, code: &str) -> Result<String, AuthError> {
        let mut guard = self.inner.write().unwrap();
        let secret = guard.secret.as_ref().ok_or(AuthError::NotProvisioned)?;
        if !verify_code(secret, code) {
            return Err(AuthError::InvalidCode);
        }
        Ok(new_session_token(&mut guard))
    }

    /// Validates a session token, pruning expired entries as a side effect.
    pub fn validate_session(&self, token: &str) -> bool {
        let mut guard = self.inner.write().unwrap();
        prune(&mut guard);
        guard.sessions.contains_key(token)
    }

    pub fn destroy_session(&self, token: &str) {
        self.inner.write().unwrap().sessions.remove(token);
    }

    /// Number of live sessions, after pruning expired ones. Used by the
    /// integration tests to assert that a token is really tracked server-side.
    #[allow(dead_code)]
    pub fn session_count(&self) -> usize {
        let mut guard = self.inner.write().unwrap();
        prune(&mut guard);
        guard.sessions.len()
    }
}

pub struct SetupMaterial {
    pub secret: String,
    pub qr_svg: String,
    /// The `otpauth://` URI encoded in `qr_svg`. Kept alongside the rendered
    /// markup so the terminal setup flow can print it as text without
    /// regenerating the QR.
    pub qr_uri: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AuthError {
    InvalidCode,
    NoPendingSecret,
    NotProvisioned,
    PersistFailed,
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            AuthError::InvalidCode => {
                "That code was not accepted. Check your authenticator and try again."
            }
            AuthError::NoPendingSecret => "No setup is in progress. Reload the setup page.",
            AuthError::NotProvisioned => "This device has not been set up yet.",
            AuthError::PersistFailed => "Could not write the secret to disk.",
        };
        f.write_str(msg)
    }
}

fn build_totp(secret: Secret) -> Result<Totp, totp_rs::TotpError> {
    Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(DIGITS)
        .with_skew(SKEW)
        .with_step_duration(PERIOD)
        .with_account_name(ACCOUNT)
        .with_issuer(Some(ISSUER))
        .with_secret(secret)
        .build()
}

/// The secret must parse to a valid length before we even look at the code,
/// so a corrupt `.totp_secret` fails closed rather than accepting anything.
fn verify_code(encoded_secret: &str, code: &str) -> bool {
    let secret = match Secret::try_from_base32(encoded_secret) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let totp = match build_totp(secret) {
        Ok(t) => t,
        Err(_) => return false,
    };

    let code = code.trim().replace([' ', '-'], "");
    if code.len() != DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    totp.check_current(&code).is_some()
}

fn prune(guard: &mut Inner) {
    let now = Instant::now();
    guard.sessions.retain(|_, expiry| *expiry > now);
}

fn new_session_token(guard: &mut Inner) -> String {
    let token = generate_token();
    guard
        .sessions
        .insert(token.clone(), Instant::now() + SESSION_TTL);
    token
}

/// 32 hex-encoded bytes (64 chars) of OS-seeded randomness. `rand::rng()` is
/// a thread-local generator seeded from the operating system, which is
/// appropriate for session tokens.
fn generate_token() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    (0..32)
        .map(|_| {
            let b: u8 = rng.random();
            format!("{b:02x}")
        })
        .collect()
}

fn write_secret_file(path: &Path, secret: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, secret)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // The secret is the only thing standing between a stolen filesystem
        // and a forged TOTP stream.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_secret_path(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!(
            "eternalibre-auth-{}-{}-{}",
            tag,
            std::process::id(),
            n
        ));
        p.set_extension("secret");
        let _ = std::fs::remove_file(&p);
        p
    }

    /// A TOTP for the same secret the AuthState holds, i.e. what a real
    /// authenticator app displays right now.
    fn current_code(encoded_secret: &str) -> String {
        let totp = build_totp(Secret::try_from_base32(encoded_secret).unwrap()).unwrap();
        totp.generate_current().to_string()
    }

    #[test]
    fn fresh_state_needs_setup() {
        let path = temp_secret_path("fresh");
        let auth = AuthState::with_secret_path(&path);
        assert!(auth.needs_setup());
        assert_eq!(auth.session_count(), 0);
    }

    #[test]
    fn setup_material_contains_qr_and_secret() {
        let path = temp_secret_path("material");
        let auth = AuthState::with_secret_path(&path);
        let material = auth.begin_setup().unwrap();
        assert!(material.qr_svg.contains("<svg"));
        assert!(material.secret.len() >= 16);
    }

    #[test]
    fn setup_commits_secret_and_issues_session() {
        let path = temp_secret_path("setup");
        let auth = AuthState::with_secret_path(&path);
        let material = auth.begin_setup().unwrap();

        let token = auth.verify_setup(&current_code(&material.secret)).unwrap();

        assert!(!auth.needs_setup());
        assert!(auth.validate_session(&token));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().trim(),
            material.secret
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn setup_rejects_wrong_code_and_writes_nothing() {
        let path = temp_secret_path("badsetup");
        let auth = AuthState::with_secret_path(&path);
        auth.begin_setup().unwrap();

        assert_eq!(auth.verify_setup("000000"), Err(AuthError::InvalidCode));
        assert!(auth.needs_setup(), "secret must not commit on failure");
        assert!(!path.exists(), "no file should be written on failure");
    }

    #[test]
    fn login_verifies_against_stored_secret() {
        let path = temp_secret_path("login");
        let auth = AuthState::with_secret_path(&path);
        let material = auth.begin_setup().unwrap();
        auth.verify_setup(&current_code(&material.secret)).unwrap();

        let token = auth.verify_login(&current_code(&material.secret)).unwrap();
        assert!(auth.validate_session(&token));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn login_rejects_wrong_code() {
        let path = temp_secret_path("badlogin");
        let auth = AuthState::with_secret_path(&path);
        let material = auth.begin_setup().unwrap();
        auth.verify_setup(&current_code(&material.secret)).unwrap();

        assert_eq!(auth.verify_login("111111"), Err(AuthError::InvalidCode));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn login_before_setup_is_rejected() {
        let path = temp_secret_path("nosetup");
        let auth = AuthState::with_secret_path(&path);
        assert_eq!(auth.verify_login("123456"), Err(AuthError::NotProvisioned));
    }

    #[test]
    fn malformed_codes_are_rejected_without_panicking() {
        let path = temp_secret_path("malformed");
        let auth = AuthState::with_secret_path(&path);
        let material = auth.begin_setup().unwrap();
        auth.verify_setup(&current_code(&material.secret)).unwrap();

        for bad in [
            "",
            "   ",
            "abcdef",
            "12345",
            "1234567",
            "12 34 56 78",
            "𝟙𝟚𝟛𝟒𝟓𝟔",
        ] {
            assert_eq!(
                auth.verify_login(bad),
                Err(AuthError::InvalidCode),
                "expected {bad:?} to be rejected"
            );
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn spaced_codes_are_accepted() {
        let path = temp_secret_path("spaced");
        let auth = AuthState::with_secret_path(&path);
        let material = auth.begin_setup().unwrap();
        auth.verify_setup(&current_code(&material.secret)).unwrap();

        let code = current_code(&material.secret);
        let spaced = format!("{} {}", &code[..3], &code[3..]);
        let token = auth.verify_login(&spaced).unwrap();
        assert!(auth.validate_session(&token));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupt_secret_file_fails_closed() {
        let path = temp_secret_path("corrupt");
        std::fs::write(&path, "not!valid!base32!!").unwrap();
        let auth = AuthState::with_secret_path(&path);

        // Provisioned (a file exists) but unusable, so login can never pass.
        for code in ["000000", "123456", "999999"] {
            assert_eq!(auth.verify_login(code), Err(AuthError::InvalidCode));
        }
        assert_eq!(auth.session_count(), 0);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unknown_session_token_is_invalid() {
        let path = temp_secret_path("unknown");
        let auth = AuthState::with_secret_path(&path);
        assert!(!auth.validate_session("not-a-real-token"));
    }

    #[test]
    fn destroyed_session_stops_validating() {
        let path = temp_secret_path("destroy");
        let auth = AuthState::with_secret_path(&path);
        let material = auth.begin_setup().unwrap();
        let token = auth.verify_setup(&current_code(&material.secret)).unwrap();

        assert!(auth.validate_session(&token));
        auth.destroy_session(&token);
        assert!(!auth.validate_session(&token));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn expired_sessions_are_pruned() {
        let path = temp_secret_path("expire");
        let auth = AuthState::with_secret_path(&path);
        {
            let mut guard = auth.inner.write().unwrap();
            guard
                .sessions
                .insert("stale".to_string(), Instant::now() - Duration::from_secs(1));
            guard
                .sessions
                .insert("fresh".to_string(), Instant::now() + SESSION_TTL);
        }
        assert!(auth.validate_session("fresh"));
        assert!(!auth.validate_session("stale"));
        assert_eq!(auth.session_count(), 1);
    }

    #[test]
    fn deleting_secret_file_reopens_setup() {
        let path = temp_secret_path("recover");
        let auth = AuthState::with_secret_path(&path);
        let material = auth.begin_setup().unwrap();
        auth.verify_setup(&current_code(&material.secret)).unwrap();
        assert!(!auth.needs_setup());

        std::fs::remove_file(&path).unwrap();
        let reopened = AuthState::with_secret_path(&path);
        assert!(
            reopened.needs_setup(),
            "removing .totp_secret should allow a fresh setup"
        );
    }

    #[test]
    fn regenerating_setup_replaces_the_pending_secret() {
        let path = temp_secret_path("regen");
        let auth = AuthState::with_secret_path(&path);
        let first = auth.begin_setup().unwrap().secret;
        let second = auth.begin_setup().unwrap().secret;
        assert_ne!(first, second);

        // The QR code actually on screen is the one that must work.
        let token = auth.verify_setup(&current_code(&second)).unwrap();
        assert!(auth.validate_session(&token));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn each_setup_generates_a_distinct_secret() {
        let a = build_totp(Secret::generate()).unwrap().secret().to_base32();
        let b = build_totp(Secret::generate()).unwrap().secret().to_base32();
        assert_ne!(a, b);
    }

    #[test]
    fn session_tokens_are_unique_and_long() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 64, "32 bytes hex-encoded");
    }

    /// Confirms the ±1 step window: a code from the previous or next 30s
    /// window must still be accepted, since phone clocks drift.
    #[test]
    fn neighbouring_time_steps_are_accepted() {
        let path = temp_secret_path("skew");
        let auth = AuthState::with_secret_path(&path);
        let material = auth.begin_setup().unwrap();
        let totp = build_totp(Secret::try_from_base32(&material.secret).unwrap()).unwrap();
        auth.verify_setup(&current_code(&material.secret)).unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // skew = 1 means exactly one 30s step either side, not two.
        for offset in [-30i64, 30] {
            let code = totp.generate((now as i64 + offset) as u64).to_string();
            assert!(
                verify_code(&material.secret, &code),
                "code from {offset}s should be accepted"
            );
        }

        let _ = std::fs::remove_file(&path);
    }

    /// Two windows away is outside the ±1 skew and must be rejected, so the
    /// tolerance is not accidentally wider than intended.
    #[test]
    fn distant_time_steps_are_rejected() {
        let path = temp_secret_path("skew-far");
        let auth = AuthState::with_secret_path(&path);
        let material = auth.begin_setup().unwrap();
        let totp = build_totp(Secret::try_from_base32(&material.secret).unwrap()).unwrap();
        auth.verify_setup(&current_code(&material.secret)).unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Two or more steps out is outside the ±1 window.
        for offset in [-60i64, 60, -300, 300] {
            let code = totp.generate((now as i64 + offset) as u64).to_string();
            assert!(
                !verify_code(&material.secret, &code),
                "code from {offset}s is outside the skew window and must be rejected"
            );
        }

        let _ = std::fs::remove_file(&path);
    }
}
