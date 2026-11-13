use prism_store::Store;
use std::io::Write;
use std::sync::Arc;

/// Serve the UI until Ctrl-C. `query` is a deep-link query such as `profile=<id>` or `session=<id>`.
pub async fn run(
    store: Arc<Store>,
    port: u16,
    query: Option<String>,
    no_browser: bool,
) -> Result<(), String> {
    let (addr, handle) = prism_server::serve(store, port)
        .await
        .map_err(|e| format!("cannot bind 127.0.0.1:{port}: {e}"))?;
    let url = match query {
        Some(q) => format!("http://127.0.0.1:{}/?{q}", addr.port()),
        None => format!("http://127.0.0.1:{}/", addr.port()),
    };
    println!("prism UI: {url}");
    let _ = std::io::stdout().flush();
    if !no_browser {
        if let Err(e) = open::that(&url) {
            eprintln!("warning: could not open browser: {e}");
        }
    }
    tokio::signal::ctrl_c().await.map_err(|e| e.to_string())?;
    handle.abort();
    Ok(())
}
