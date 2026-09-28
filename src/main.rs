mod markdown;
mod notes;
mod pages;
mod routes;
mod storage;
mod themes;

use axum::Router;
use clap::Parser;
use std::net::SocketAddr;
use tower_http::services::ServeDir;

#[derive(Parser, Debug)]
#[command(name = "rasuti-notes")]
#[command(about = "A server-side rendered Markdown notes application")]
struct Args {
    /// Host address to bind to
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    /// Port to listen on
    #[arg(short, long, default_value_t = 3000)]
    port: u16,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Ensure notes directory exists
    std::fs::create_dir_all("notes").expect("Failed to create notes directory");

    let app = Router::new()
        .merge(routes::router())
        .nest_service("/static", ServeDir::new("static"));

    let host: std::net::IpAddr = args.host.parse().expect("Invalid host address");
    let addr = SocketAddr::from((host, args.port));

    println!("Rasuti Notes running at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
