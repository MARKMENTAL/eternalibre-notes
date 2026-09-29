use qrcode::render::unicode::Dense1x2;
use qrcode::QrCode;
use std::io::{BufRead, Write};

use crate::auth::{AuthError, AuthState, SetupMaterial};

/// First-run TOTP provisioning.
///
/// This runs in the terminal *before* the listener binds, so the secret is
/// never sent over the network. That is the entire reason it lives here
/// rather than on a web page: an HTTP-reachable `/setup` would let anyone
/// who can reach the port before you finish claim the device.
pub fn run_interactive_setup(auth: &AuthState) -> Result<(), SetupError> {
    let material = auth.begin_setup().map_err(SetupError::Generate)?;
    print_instructions(&material);
    flush();

    let stdin = std::io::stdin();
    let mut line = String::new();

    loop {
        print!("  Code: ");
        flush();

        line.clear();
        let read = stdin
            .lock()
            .read_line(&mut line)
            .map_err(|e| SetupError::Io(e.to_string()))?;

        if read == 0 {
            // EOF: the process was closed or stdin was closed out from under us.
            return Err(SetupError::Aborted);
        }

        match auth.verify_setup(line.trim()) {
            Ok(_) => {
                println!("\n  Verified. Secret saved to the secret file (mode 0600).");
                println!("  Delete that file and restart to enrol a new device.\n");
                return Ok(());
            }
            Err(AuthError::InvalidCode) => {
                println!("  That code was not accepted. Waiting for the next one.");
            }
            Err(e) => return Err(SetupError::Auth(e)),
        }
    }
}

fn print_instructions(material: &SetupMaterial) {
    println!("\n  EternaLibre Notes — first-run setup\n");
    println!("  Add this to an authenticator app, then type the code it shows.\n");

    print_qr(&material.qr_uri);

    println!("\n  Can't scan? Enter this key by hand:");
    println!("  {}\n", group_secret(&material.secret));
    println!("  {}\n", material.qr_uri);
}

fn print_qr(uri: &str) {
    let code = match QrCode::new(uri.as_bytes()) {
        Ok(c) => c,
        // The URI is also printed as text below, so a failure here is cosmetic.
        Err(_) => return,
    };

    // Dense1x2 packs two pixel rows per character cell, so a terminal QR
    // stays roughly square. `Dark` renders filled blocks for dark modules,
    // which reads correctly on the light background most terminals use.
    let rendered = code
        .render::<Dense1x2>()
        .min_dimensions(33, 33)
        .quiet_zone(true)
        .build();

    for line in rendered.lines() {
        println!("  {line}");
    }
}

/// Groups the Base32 secret into 4-character blocks so it can be typed or
/// transcribed without losing your place.
fn group_secret(secret: &str) -> String {
    secret
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" ")
}

fn flush() {
    let _ = std::io::stdout().flush();
}

#[derive(Debug)]
pub enum SetupError {
    Generate(String),
    Io(String),
    Auth(AuthError),
    Aborted,
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetupError::Generate(e) => write!(f, "Could not generate a TOTP secret: {e}"),
            SetupError::Io(e) => write!(f, "Could not read from the terminal: {e}"),
            SetupError::Auth(e) => write!(f, "{e}"),
            SetupError::Aborted => {
                f.write_str("Setup was cancelled before a valid code was entered.")
            }
        }
    }
}

impl std::error::Error for SetupError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_is_grouped_for_manual_entry() {
        assert_eq!(group_secret("JBSWY3DPEHPK3PXP"), "JBSW Y3DP EHPK 3PXP");
    }

    #[test]
    fn grouping_handles_odd_lengths_without_panicking() {
        assert_eq!(group_secret("ABC"), "ABC");
        assert_eq!(group_secret("ABCDE"), "ABCD E");
        assert_eq!(group_secret(""), "");
    }

    #[test]
    fn qr_renders_as_block_characters() {
        let code =
            QrCode::new(b"otpauth://totp/EternaLibre:local?secret=JBSWY3DPEHPK3PXP").unwrap();
        let out = code
            .render::<Dense1x2>()
            .min_dimensions(33, 33)
            .quiet_zone(true)
            .build();
        assert!(out.contains('\u{2588}'), "expected block characters");
        assert!(out.lines().count() > 5, "expected a multi-line QR");
    }
}
