//! Minimal local daemon binary for 0.02-S5 development smoke tests.

use splendor_daemon::{router, DaemonState};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let state = DaemonState::local_dev();
    let app = router(state);
    let listener = TcpListener::bind("127.0.0.1:8077").await?;
    eprintln!(
        "WARNING: Splendor runtime daemon is running in explicit local-only insecure dev mode on 127.0.0.1:8077"
    );
    axum::serve(listener, app).await?;
    Ok(())
}
