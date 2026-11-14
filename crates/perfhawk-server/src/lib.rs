//! Localhost JSON API and embedded UI for perfhawk.

mod api;
mod assets;
pub mod doctor;
mod guard;
mod jobs;

use axum::Router;
use perfhawk_capture::ProviderRegistry;
use perfhawk_model::Profile;
use perfhawk_store::Store;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;

pub(crate) struct AppState {
    pub store: Arc<Store>,
    pub registry: Arc<ProviderRegistry>,
    pub jobs: jobs::JobManager,
    pub port: Option<u16>,
    pub cache: Mutex<lru::LruCache<String, Arc<Profile>>>,
    /// Timeline sidecars (None = the profile has no usable timeline).
    pub timelines: Mutex<lru::LruCache<String, Option<Arc<perfhawk_model::Timeline>>>>,
}

/// Router accepting loopback hosts on any port (use `serve` for a strict port).
pub fn router(store: Arc<Store>) -> Router {
    router_with_port(store, None, Arc::new(ProviderRegistry::with_defaults()))
}

fn router_with_port(
    store: Arc<Store>,
    port: Option<u16>,
    registry: Arc<ProviderRegistry>,
) -> Router {
    let state = Arc::new(AppState {
        store,
        registry,
        jobs: jobs::JobManager::default(),
        port,
        cache: Mutex::new(lru::LruCache::new(NonZeroUsize::new(2).unwrap())),
        timelines: Mutex::new(lru::LruCache::new(NonZeroUsize::new(8).unwrap())),
    });
    api::routes()
        .fallback(assets::static_handler)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            guard::host_origin_guard,
        ))
        .layer(axum::middleware::from_fn(guard::security_headers))
        .with_state(state)
}

/// Bind `127.0.0.1:port` (0 = random) and serve in a background task.
/// Must be called inside a tokio runtime.
pub async fn serve(
    store: Arc<Store>,
    port: u16,
) -> Result<(SocketAddr, JoinHandle<()>), std::io::Error> {
    serve_with_registry(store, port, Arc::new(ProviderRegistry::with_defaults())).await
}

/// Like [`serve`] with the capture providers to use (tests inject fakes).
pub async fn serve_with_registry(
    store: Arc<Store>,
    port: u16,
    registry: Arc<ProviderRegistry>,
) -> Result<(SocketAddr, JoinHandle<()>), std::io::Error> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let addr = listener.local_addr()?;
    let app = router_with_port(store, Some(addr.port()), registry);
    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("perfhawk-server: {e}");
        }
    });
    Ok((addr, handle))
}
