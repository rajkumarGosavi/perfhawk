use crate::AppState;
use axum::{
    body::Bytes,
    extract::{rejection::BytesRejection, DefaultBodyLimit, FromRequestParts, Path, Query, State},
    http::{header, request::Parts, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use prism_analysis::{
    call_graph, diff_flame, diff_top_sorted, findings, flame_with, function_stats, groups,
    inverted_flame_with, match_sample_type, parse_transforms, sandwich, top, DiffError, DiffInput,
    DiffNode, DiffNormalize, DiffRow, DiffSort, Filter, FlameOptions, GroupBy, RuleConfig, TopSort,
};
use prism_model::{FuncId, Profile, ProfileKind};
use prism_store::{ImportOptions, ProfileRecord, SampleTypeInfo, StoreError};
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
        .route(
            "/api/profiles/{reference}",
            get(get_one).delete(delete_profile),
        )
        .route("/api/sessions", get(list_sessions))
        .route(
            "/api/sessions/{reference}",
            get(get_session).delete(delete_session),
        )
        .route("/api/sessions/{reference}/pivot", get(pivot_h))
        .route("/api/diff", get(diff_h))
        .route("/api/profiles/{reference}/flame", get(flame_h))
        .route("/api/profiles/{reference}/top", get(top_h))
        .route("/api/profiles/{reference}/sandwich", get(sandwich_h))
        .route("/api/profiles/{reference}/groups", get(groups_h))
        .route("/api/profiles/{reference}/findings", get(findings_h))
        .route("/api/profiles/{reference}/graph", get(graph_h))
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

/// Deletes need `X-Prism-Request: 1`: a custom header forces a CORS preflight,
/// which prism never approves, so other web pages cannot delete data.
fn require_custom_header(h: &HeaderMap) -> ApiResult<()> {
    if h.get("x-prism-request").and_then(|v| v.to_str().ok()) == Some("1") {
        Ok(())
    } else {
        Err(ApiError::forbidden("missing X-Prism-Request header"))
    }
}

async fn delete_profile(
    State(s): State<St>,
    Path(r): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    require_custom_header(&headers)?;
    let store = s.store.clone();
    let id = blocking(move || {
        let rec = store.resolve(&r)?;
        store.remove_profile(&rec.id)?;
        Ok(rec.id)
    })
    .await?;
    s.cache.lock().unwrap().pop(&id);
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn list_sessions(State(s): State<St>) -> ApiResult<Response> {
    let store = s.store.clone();
    Ok(Json(blocking(move || store.list_sessions()).await?).into_response())
}

async fn get_session(State(s): State<St>, Path(r): Path<String>) -> ApiResult<Response> {
    let store = s.store.clone();
    Ok(Json(blocking(move || store.resolve_session(&r)).await?).into_response())
}

async fn delete_session(
    State(s): State<St>,
    Path(r): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    require_custom_header(&headers)?;
    let store = s.store.clone();
    let (ids, removed) = blocking(move || {
        let sess = store.resolve_session(&r)?;
        let removed = store.remove_session(&sess.id)?;
        Ok((sess.profile_ids, removed))
    })
    .await?;
    {
        let mut cache = s.cache.lock().unwrap();
        for id in &ids {
            cache.pop(id);
        }
    }
    Ok(Json(json!({ "removed": removed })).into_response())
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
        ..Default::default()
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
    sort: Option<String>,
    depth: Option<usize>,
    tx: Option<String>,
    by: Option<String>,
    nodes: Option<usize>,
}

const DEFAULT_DEPTH: usize = 1000;
const MAX_DEPTH: usize = 10_000;

fn check_depth(depth: Option<usize>) -> ApiResult<usize> {
    let d = depth.unwrap_or(DEFAULT_DEPTH);
    if !(1..=MAX_DEPTH).contains(&d) {
        return Err(ApiError::bad(format!(
            "depth must be within 1..={MAX_DEPTH}"
        )));
    }
    Ok(d)
}

fn build_filter(
    focus: Option<&str>,
    exclude: Option<&str>,
    hide: Option<&str>,
    tx: Option<&str>,
) -> ApiResult<Filter> {
    let f = Filter::parse(focus, exclude, hide).map_err(|e| ApiError::bad(e.to_string()))?;
    let t = parse_transforms(tx.unwrap_or_default()).map_err(|e| ApiError::bad(e.to_string()))?;
    Ok(f.with_transforms(t))
}

impl AnalysisQuery {
    fn filter(&self) -> ApiResult<Filter> {
        build_filter(
            self.focus.as_deref(),
            self.exclude.as_deref(),
            self.hide.as_deref(),
            self.tx.as_deref(),
        )
    }
    fn sort(&self) -> ApiResult<TopSort> {
        match self.sort.as_deref() {
            None | Some("self") => Ok(TopSort::SelfValue),
            Some("total") => Ok(TopSort::Total),
            Some(o) => Err(ApiError::bad(format!(
                "unknown sort '{o}' (expected self or total)"
            ))),
        }
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
    let depth = check_depth(q.depth)?;
    let p = load_profile(&s, r).await?;
    let st = q.sample_type(&p)?;
    let inv = q.inverted.unwrap_or(false);
    let opts = FlameOptions {
        min_fraction: min,
        max_depth: Some(depth),
    };
    tokio::task::spawn_blocking(move || {
        let node = if inv {
            inverted_flame_with(&p, st, &filter, &opts)
        } else {
            flame_with(&p, st, &filter, &opts)
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
    let sort = q.sort()?;
    let p = load_profile(&s, r).await?;
    let st = q.sample_type(&p)?;
    tokio::task::spawn_blocking(move || json_bytes(&top(&p, st, &filter, limit, sort)))
        .await
        .map_err(ApiError::internal)?
}

async fn groups_h(
    State(s): State<St>,
    Path(r): Path<String>,
    Q(q): Q<AnalysisQuery>,
) -> ApiResult<Response> {
    let filter = q.filter()?;
    let by = match q.by.as_deref() {
        Some("stack") => GroupBy::Stack,
        Some("site") => GroupBy::Site,
        Some(o) => {
            return Err(ApiError::bad(format!(
                "unknown by '{o}' (expected stack or site)"
            )))
        }
        None => return Err(ApiError::bad("missing required query parameter 'by'")),
    };
    let limit = q.limit.unwrap_or(100).min(1000);
    let p = load_profile(&s, r).await?;
    let st = q.sample_type(&p)?;
    tokio::task::spawn_blocking(move || json_bytes(&groups(&p, st, &filter, by, limit)))
        .await
        .map_err(ApiError::internal)?
}

async fn findings_h(
    State(s): State<St>,
    Path(r): Path<String>,
    Q(q): Q<AnalysisQuery>,
) -> ApiResult<Response> {
    let filter = q.filter()?;
    let p = load_profile(&s, r).await?;
    let st = q.sample_type(&p)?;
    tokio::task::spawn_blocking(move || {
        json_bytes(&findings(&p, st, &filter, &RuleConfig::default()))
    })
    .await
    .map_err(ApiError::internal)?
}

async fn graph_h(
    State(s): State<St>,
    Path(r): Path<String>,
    Q(q): Q<AnalysisQuery>,
) -> ApiResult<Response> {
    let filter = q.filter()?;
    let nodes = match q.nodes {
        None => 80,
        Some(0) => return Err(ApiError::bad("nodes must be at least 1")),
        Some(n) => n.min(300),
    };
    let p = load_profile(&s, r).await?;
    let st = q.sample_type(&p)?;
    tokio::task::spawn_blocking(move || json_bytes(&call_graph(&p, st, &filter, nodes)))
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

#[derive(Deserialize, Default)]
struct DiffQuery {
    base: Option<String>,
    head: Option<String>,
    st: Option<String>,
    normalize: Option<String>,
    view: Option<String>,
    min: Option<f64>,
    depth: Option<usize>,
    limit: Option<usize>,
    focus: Option<String>,
    exclude: Option<String>,
    hide: Option<String>,
    tx: Option<String>,
    sort: Option<String>,
}

#[derive(serde::Serialize)]
struct SampleTypeOut {
    name: String,
    unit: String,
}

#[derive(serde::Serialize)]
struct DiffFlameOut<'a> {
    base: &'a ProfileRecord,
    head: &'a ProfileRecord,
    #[serde(rename = "sampleType")]
    sample_type: SampleTypeOut,
    root: DiffNode,
}

#[derive(serde::Serialize)]
struct DiffTopOut<'a> {
    base: &'a ProfileRecord,
    head: &'a ProfileRecord,
    #[serde(rename = "sampleType")]
    sample_type: SampleTypeOut,
    rows: Vec<DiffRow>,
}

async fn record_and_profile(s: &St, reference: String) -> ApiResult<(ProfileRecord, Arc<Profile>)> {
    let store = s.store.clone();
    let r2 = reference.clone();
    let rec = blocking(move || store.resolve(&r2)).await?;
    let p = load_profile(s, rec.id.clone()).await?;
    Ok((rec, p))
}

async fn diff_h(State(s): State<St>, Q(q): Q<DiffQuery>) -> ApiResult<Response> {
    let base_ref = q
        .base
        .clone()
        .ok_or_else(|| ApiError::bad("missing required query parameter 'base'"))?;
    let head_ref = q
        .head
        .clone()
        .ok_or_else(|| ApiError::bad("missing required query parameter 'head'"))?;
    let normalize = match q.normalize.as_deref() {
        None | Some("none") => DiffNormalize::None,
        Some("total") => DiffNormalize::Total,
        Some(o) => {
            return Err(ApiError::bad(format!(
                "unknown normalize '{o}' (expected none or total)"
            )))
        }
    };
    let top_view = match q.view.as_deref() {
        None | Some("flame") => false,
        Some("top") => true,
        Some(o) => {
            return Err(ApiError::bad(format!(
                "unknown view '{o}' (expected flame or top)"
            )))
        }
    };
    let min = {
        let m = q.min.unwrap_or(0.0005);
        if m.is_nan() || !(0.0..=1.0).contains(&m) {
            return Err(ApiError::bad("min must be within 0..=1"));
        }
        m
    };
    let sort = match q.sort.as_deref() {
        None | Some("delta") => DiffSort::Delta,
        Some("percent") => DiffSort::Percent,
        Some(o) => {
            return Err(ApiError::bad(format!(
                "unknown sort '{o}' (expected delta or percent)"
            )))
        }
    };
    let depth = check_depth(q.depth)?;
    let limit = q.limit.unwrap_or(100).min(10_000);
    let filter = build_filter(
        q.focus.as_deref(),
        q.exclude.as_deref(),
        q.hide.as_deref(),
        q.tx.as_deref(),
    )?;

    let (brec, bp) = record_and_profile(&s, base_ref).await?;
    let (hrec, hp) = record_and_profile(&s, head_ref).await?;
    if let Some(n) = &q.st {
        if bp.sample_type_index(n).is_none() || hp.sample_type_index(n).is_none() {
            return Err(ApiError::bad(format!(
                "sample type '{n}' is not present in both profiles"
            )));
        }
    }
    let (bi, hi) = match_sample_type(&bp, &hp, q.st.as_deref()).map_err(|e| match e {
        DiffError::NoCommonSampleType { .. } => ApiError::bad(e.to_string()),
    })?;
    let info: SampleTypeInfo = brec.sample_types[bi].clone();
    let sample_type = SampleTypeOut {
        name: info.name,
        unit: info.unit,
    };
    tokio::task::spawn_blocking(move || {
        let base = DiffInput {
            profile: &bp,
            sample_type: bi,
        };
        let head = DiffInput {
            profile: &hp,
            sample_type: hi,
        };
        if top_view {
            let rows = diff_top_sorted(base, head, &filter, normalize, limit, sort);
            json_bytes(&DiffTopOut {
                base: &brec,
                head: &hrec,
                sample_type,
                rows,
            })
        } else {
            let root = diff_flame(base, head, &filter, normalize, min, Some(depth));
            json_bytes(&DiffFlameOut {
                base: &brec,
                head: &hrec,
                sample_type,
                root,
            })
        }
    })
    .await
    .map_err(ApiError::internal)?
}

#[derive(Deserialize, Default)]
struct PivotQuery {
    name: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PivotRow {
    profile_id: String,
    short_id: String,
    kind: ProfileKind,
    name: String,
    sample_type: SampleTypeOut,
    found: bool,
    func_id: Option<u32>,
    self_value: i64,
    total: i64,
    self_pct: f64,
    total_pct: f64,
}

async fn pivot_h(
    State(s): State<St>,
    Path(r): Path<String>,
    Q(q): Q<PivotQuery>,
) -> ApiResult<Response> {
    let name = q
        .name
        .filter(|n| !n.is_empty())
        .ok_or_else(|| ApiError::bad("missing required query parameter 'name'"))?;
    let store = s.store.clone();
    let rows = blocking(move || {
        let sess = store.resolve_session(&r)?;
        let filter = Filter::default();
        let mut out = Vec::new();
        for id in &sess.profile_ids {
            let rec = store.resolve(id)?;
            let p = store.load(id)?;
            let st = rec.default_sample_type;
            let info = rec.sample_types.get(st).cloned();
            let hit = function_stats(&p, st, &filter, &name);
            out.push(PivotRow {
                profile_id: rec.id.clone(),
                short_id: rec.short_id.clone(),
                kind: rec.kind,
                name: name.clone(),
                sample_type: SampleTypeOut {
                    name: info.as_ref().map(|i| i.name.clone()).unwrap_or_default(),
                    unit: info.map(|i| i.unit).unwrap_or_default(),
                },
                found: hit.is_some(),
                func_id: hit.as_ref().map(|h| h.func_id),
                self_value: hit.as_ref().map_or(0, |h| h.self_value),
                total: hit.as_ref().map_or(0, |h| h.total),
                self_pct: hit.as_ref().map_or(0.0, |h| h.self_pct),
                total_pct: hit.as_ref().map_or(0.0, |h| h.total_pct),
            });
        }
        Ok(out)
    })
    .await?;
    Ok(Json(rows).into_response())
}
