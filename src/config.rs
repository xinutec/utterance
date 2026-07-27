//! Runtime configuration, read from the environment at startup.

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    /// Address to bind the HTTP server to.
    pub bind_addr: String,
    /// Where recordings and their voiceprints live.
    pub data_dir: PathBuf,
    /// Directory of the built Angular bundle to serve, with SPA fallback. Unset
    /// in dev, where `ng serve` proxies `/api` here and serves the app itself.
    pub static_dir: Option<PathBuf>,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8181".to_string()),
            data_dir: std::env::var("DATA_DIR")
                .map_or_else(|_| PathBuf::from("data"), PathBuf::from),
            static_dir: std::env::var("STATIC_DIR").ok().map(PathBuf::from),
        }
    }
}
