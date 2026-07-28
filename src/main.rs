//! utterance — derive music from the structure of a voice.
//! Entry point: open the store, build the router, serve.

use anyhow::{Context, Result};
use tracing_subscriber::EnvFilter;
use utterance::{
    config::{Config, Invocation, invocation},
    routes,
    state::AppState,
    store::Store,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Read before anything is opened or bound, and before the logger is set up.
    // Someone asking what the flags are should get an answer rather than a
    // startup log, and a bad argument should cost nothing.
    match invocation(std::env::args().skip(1)) {
        Ok(Invocation::Print(text)) => {
            println!("{text}");
            return Ok(());
        }
        Err(complaint) => {
            eprintln!("{complaint}");
            std::process::exit(2);
        }
        Ok(Invocation::Serve) => {}
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = Config::from_env();
    let store = Store::open(&cfg.data_dir)
        .with_context(|| format!("opening the recording store at {}", cfg.data_dir.display()))?;

    tracing::info!("recordings in {}", cfg.data_dir.display());
    match &cfg.static_dir {
        Some(dir) => tracing::info!("serving the frontend from {}", dir.display()),
        None => tracing::info!("API only — run the frontend with `ng serve`"),
    }

    let bind_addr = cfg.bind_addr.clone();
    let app = routes::router(AppState::new(cfg, store));

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("binding {bind_addr}"))?;
    tracing::info!("utterance listening on http://{bind_addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
