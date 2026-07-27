//! Shared application state.

use std::sync::Arc;

use crate::config::Config;
use crate::store::Store;

#[derive(Clone, Debug)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub store: Arc<Store>,
}

impl AppState {
    pub fn new(cfg: Config, store: Store) -> Self {
        Self {
            cfg: Arc::new(cfg),
            store: Arc::new(store),
        }
    }
}
