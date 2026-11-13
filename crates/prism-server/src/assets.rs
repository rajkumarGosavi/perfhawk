use axum::{
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../ui/dist"]
#[allow_missing = true]
struct Assets;

const PLACEHOLDER: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>prism</title></head><body><p>UI not built \u{2014} run <code>npm run build</code> in ui/</p></body></html>";

fn file_response(path: &str) -> Option<Response> {
    let f = Assets::get(path)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Some(
        (
            [(header::CONTENT_TYPE, mime.to_string())],
            f.data.into_owned(),
        )
            .into_response(),
    )
}

/// Serve an embedded file, falling back to `index.html` (SPA) or a placeholder.
pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") || path == "api" {
        return super::api::not_found_response();
    }
    if !path.is_empty() {
        if let Some(r) = file_response(path) {
            return r;
        }
    }
    file_response("index.html").unwrap_or_else(|| {
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            PLACEHOLDER,
        )
            .into_response()
    })
}
