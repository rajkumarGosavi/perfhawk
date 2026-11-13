pub mod import;
pub mod ls;
pub mod serve;
pub mod show;

/// snake_case serde string of a unit-variant enum (e.g. `block_io`).
pub fn kind_str<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}
