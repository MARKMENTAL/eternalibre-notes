use clap::Parser;
use eternalibre_notes::routes::{self, AppState};
use eternalibre_notes::{auth, notes, setup, syntax};
use std::net::SocketAddr;

#[derive(Parser, Debug)]
#[command(name = "eternalibre-notes")]
#[command(about = "A server-side rendered Markdown notes application")]
struct Args {
    /// Host address to bind to
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    /// Port to listen on
    #[arg(short, long, default_value_t = 3000)]
    port: u16,

    /// Directory holding note files
    #[arg(long, default_value = "notes")]
    notes_dir: String,

    /// Path prefix when served behind a reverse proxy subdirectory.
    /// Example: --base-path /forgejo when the app answers at
    /// https://host/forgejo/. Leave empty when mounted at the site root.
    #[arg(long, default_value = "")]
    base_path: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Route note storage through the env var the library reads, so the binary
    // flag and the library agree on one location.
    std::env::set_var("ETERNALIBRE_NOTES_DIR", &args.notes_dir);

    std::fs::create_dir_all(notes::notes_dir()).expect("Failed to create notes directory");

    // Eagerly initialize syntax highlighting so the first request isn't slow.
    let _ = syntax::syntax_state();

    let auth = std::sync::Arc::new(auth::AuthState::new());

    // First-run TOTP enrolment happens here, on the terminal, before the
    // listener binds. Nothing is listening yet, so there is no window in
    // which an unauthenticated peer could claim the device.
    if auth.needs_setup() {
        if let Err(e) = setup::run_interactive_setup(&auth) {
            eprintln!("\n  Setup failed: {e}");
            std::process::exit(1);
        }
    } else {
        println!("TOTP is enabled; a code is required on every visit.");
    }

    let base_path = routes::normalize_base_path(&args.base_path);
    if !base_path.is_empty() {
        println!("Serving under base path {base_path}");
    }

    let state = AppState {
        auth,
        base_path: base_path.into(),
    };
    let base = state.base_path.to_string();

    // NOTE: static assets are mounted app-absolute by `routes::app`, never
    // under the base path. A reverse proxy strips the prefix before
    // forwarding, so `/forgejo/static/app.js` arrives here as
    // `/static/app.js`. See the comment on `routes::app` for the full
    // invariant and the failure it causes when broken.
    let app = routes::app(state);

    let host: std::net::IpAddr = args.host.parse().expect("Invalid host address");
    let addr = SocketAddr::from((host, args.port));

    let url = if base.is_empty() {
        format!("http://{addr}")
    } else {
        format!("http://{addr}{base}/")
    };
    println!("EternaLibre Notes on {url}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
