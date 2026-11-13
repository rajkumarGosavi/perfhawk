use crate::AppState;
use axum::{
    body::Bytes,
    extract::{rejection::BytesRejection, DefaultBodyLimit, FromRequestParts, Path, Query, State},
    http::{header, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use prism_analysis::{flame, inverted_flame, sandwich, top, Filter};
use prism_model::{FuncId, Profile, ProfileKind};
use prism_store::{ImportOptions, StoreError};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

type St = Arc<AppState>;
type ApiResult<T> = Result<T, ApiError>;

pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn bad(message: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::BAD_REQUEST,
            code: "bad_request",
            message: message.into(),
        }
    }
    fn internal(detail: impl std::fmt::Display) -> Self {
        eprintln!("prism-server: internal error: {detail}");
        ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal",
            message: "internal error".into(),
        }
    }
    pub fn forbidden(message: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::FORBIDDEN,
            code: "forbidden",
            message: message.into(),
        }
    }
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        let (status, code) = match &e {
            StoreError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            StoreError::Ambiguous { .. } => (StatusCode::CONFLICT, "ambiguous"),
            StoreError::Import(_) => (StatusCode::UNPROCESSABLE_ENTITY, "import_failed"),
            _ => return ApiError::internal(e),
        };
        ApiError {
            status,
            code,
            message: e.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({"error": {"code": self.code, "message": self.message}})),
        )
            .into_response()
    }
}

/// `axum::extract::Query` with JSON `bad_request` rejections.
pub struct Q<T>(pub T);

impl<T, S> FromRequestParts<S> for Q<T>
where
    T: serde::de::DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiError> {
        Query::<T>::from_request_parts(parts, state)
            .await
            .map(|Query(v)| Q(v))
            .map_err(|e| ApiError::bad(e.body_text()))
    }
}

pub fn not_found_response() -> Response {
    ApiError {
        status: StatusCode::NOT_FOUND,
        code: "not_found",
        message: "no such endpoint".into(),
    }
    .into_response()
}

pub fn routes() -> Router<St> {
    Router::new()
        .route("/api/profiles", get(list).post(upload))
        .route("/api/profiles/{reference}", get(get_one))
        .route("/api/profiles/{reference}/flame", get(flame_h))
        .route("/api/profiles/{reference}/top", get(top_h))
        .route("/api/profiles/{reference}/sandwich", get(sandwich_h))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024 * 1024))
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, StoreError> + Send + 'static,
) -> ApiResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::from)
}

async fn list(State(s): State<St>) -> ApiResult<Response> {
    let store = s.store.clone();
    Ok(Json(blocking(move || store.list()).await?).into_response())
}

async fn get_one(State(s): State<St>, Path(r): Path<String>) -> ApiResult<Response> {
    let store = s.store.clone();
    Ok(Json(blocking(move || store.resolve(&r)).await?).into_response())
}

#[derive(Deserialize, Default)]
struct UploadQuery {
    name: Option<String>,
    file_name: Option<String>,
    kind: Option<String>,
}

async fn upload(
    State(s): State<St>,
    Q(q): Q<UploadQuery>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<Response> {
    let body = body.map_err(|e| {
        if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError {
                status: StatusCode::PAYLOAD_TOO_LARGE,
                code: "payload_too_large",
                message: "request body too large".into(),
            }
        } else {
            ApiError::bad(e.body_text())
        }
    })?;
    let kind_override = match q.kind.as_deref() {
        None | Some("") => None,
        Some(k) => Some(
            serde_json::from_value::<ProfileKind>(serde_json::Value::String(k.to_string()))
                .map_err(|_| ApiError::bad(format!("unknown kind '{k}'")))?,
        ),
    };
    let opts = ImportOptions {
        name: q.name,
        file_name: q.file_name,
        kind_override,
        tags: vec![],
    };
    let store = s.store.clone();
    let (record, created) = blocking(move || store.import(&body, opts)).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(json!({"record": record, "created": created}))).into_response())
}

#[derive(Deserialize, Default)]
struct AnalysisQuery {
    st: Option<usize>,
    focus: Option<String>,
    exclude: Option<String>,
    hide: Option<String>,
    min: Option<f64>,
    inverted: Option<bool>,
    limit: Option<usize>,
    func: Option<u32>,
}

impl AnalysisQuery {
    fn filter(&self) -> ApiResult<Filter> {
        Filter::parse(
            self.focus.as_deref(),
            self.exclude.as_deref(),
            self.hide.as_deref(),
        )
        .map_err(|e| ApiError::bad(e.to_string()))
    }
    fn min(&self) -> ApiResult<f64> {
        let m = self.min.unwrap_or(0.0005);
        if m.is_nan() || !(0.0..=1.0).contains(&m) {
            return Err(ApiError::bad("min must be within 0..=1"));
        }
        Ok(m)
    }
    fn sample_type(&self, p: &Profile) -> ApiResult<usize> {
        let st = self.st.unwrap_or(p.default_sample_type);
        if st >= p.sample_types.len() {
            return Err(ApiError::bad(format!(
                "st {st} out of range (profile has {} sample types)",
                p.sample_types.len()
            )));
        }
        Ok(st)
    }
}

/// Resolve a reference and fetch the profile through the 2-entry LRU.
async fn load_profile(s: &St, reference: String) -> ApiResult<Arc<Profile>> {
    let store = s.store.clone();
    let id = blocking(move || store.resolve(&reference)).await?.id;
    if let Some(p) = s.cache.lock().unwrap().get(&id) {
        return Ok(p.clone());
    }
    let store = s.store.clone();
    let id2 = id.clone();
    let p = Arc::new(blocking(move || store.load(&id2)).await?);
    s.cache.lock().unwrap().put(id, p.clone());
    Ok(p)
}

fn json_bytes<T: serde::Serialize>(v: &T) -> ApiResult<Response> {
    let body = serde_json::to_vec(v).map_err(ApiError::internal)?;
    Ok(([(header::CONTENT_TYPE, "application/json")], body).into_response())
}

async fn flame_h(
    State(s): State<St>,
    Path(r): Path<String>,
    Q(q): Q<AnalysisQuery>,
) -> ApiResult<Response> {
    let filter = q.filter()?;
    let min = q.min()?;
    let p = load_profile(&s, r).await?;
    let st = q.sample_type(&p)?;
    let inv = q.inverted.unwrap_or(false);
    tokio::task::spawn_blocking(move || {
        let node = if inv {
            inverted_flame(&p, st, &filter, min)
        } else {
            flame(&p, st, &filter, min)
        };
        json_bytes(&node)
    })
    .await
    .map_err(ApiError::internal)?
}

async fn top_h(
    State(s): State<St>,
    Path(r): Path<String>,
    Q(q): Q<AnalysisQuery>,
) -> ApiResult<Response> {
    let filter = q.filter()?;
    let limit = q.limit.unwrap_or(100).min(10_000);
    let p = load_profile(&s, r).await?;
    let st = q.sample_type(&p)?;
    tokio::task::spawn_blocking(move || json_bytes(&top(&p, st, &filter, limit)))
        .await
        .map_err(ApiError::internal)?
}

async fn sandwich_h(
    State(s): State<St>,
    Path(r): Path<String>,
    Q(q): Q<AnalysisQuery>,
) -> ApiResult<Response> {
    let filter = q.filter()?;
    let min = q.min()?;
    let func = q
        .func
        .ok_or_else(|| ApiError::bad("missing required query parameter 'func'"))?;
    let p = load_profile(&s, r).await?;
    let st = q.sample_type(&p)?;
    if func as usize >= p.functions.len() {
        return Err(ApiError::bad(format!("func {func} out of range")));
    }
    tokio::task::spawn_blocking(move || json_bytes(&sandwich(&p, st, &filter, FuncId(func), min)))
        .await
        .map_err(ApiError::internal)?
}
