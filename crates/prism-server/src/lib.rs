//! Localhost JSON API and embedded UI for prism.

mod api;
mod assets;

use axum::Router;
use prism_model::Profile;
use prism_store::Store;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;

pub(crate) struct AppState {
    pub store: Arc<Store>,
    pub cache: Mutex<lru::LruCache<String, Arc<Profile>>>,
}

pub fn router(store: Arc<Store>) -> Router {
    let state = Arc::new(AppState {
        store,
        cache: Mutex::new(lru::LruCache::new(NonZeroUsize::new(2).unwrap())),
    });
    api::routes()
        .fallback(assets::static_handler)
        .with_state(state)
}

/// Bind `127.0.0.1:port` (0 = random) and serve in a background task.
/// Must be called inside a tokio runtime.
pub async fn serve(
    store: Arc<Store>,
    port: u16,
) -> Result<(SocketAddr, JoinHandle<()>), std::io::Error> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let addr = listener.local_addr()?;
    let app = router(store);
    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("prism-server: {e}");
        }
    });
    Ok((addr, handle))
}
