//! Local profile store: SQLite metadata plus zstd-compressed bincode blobs.

use prism_import::{import_bytes, ImportError, ImportHints};
use prism_model::{Fidelity, Profile, ProfileKind, SourceFormat, Unit};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("import failed: {0}")]
    Import(#[from] ImportError),
    #[error("no profile matches '{0}'")]
    NotFound(String),
    #[error("'{reference}' is ambiguous ({} matches: {})", .matches.len(), fmt_matches(.matches))]
    Ambiguous {
        reference: String,
        /// `(short_id, imported_at_unix_ms)` of every candidate.
        matches: Vec<(String, i64)>,
    },
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("corrupt store data: {0}")]
    Corrupt(String),
}

/// UTC `YYYY-MM-DD HH:MM` from unix milliseconds.
pub fn format_utc_minute(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60
    )
}

fn fmt_matches(m: &[(String, i64)]) -> String {
    m.iter()
        .map(|(id, ms)| format!("{id} {}", format_utc_minute(*ms)))
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    pub name: Option<String>,
    pub file_name: Option<String>,
    pub kind_override: Option<ProfileKind>,
    pub tags: Vec<String>,
    pub fidelity_override: Option<Fidelity>,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct SampleTypeInfo {
    pub name: String,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileRecord {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub kind: ProfileKind,
    pub source_format: SourceFormat,
    pub sample_types: Vec<SampleTypeInfo>,
    pub default_sample_type: usize,
    pub sample_count: u64,
    pub default_total: i64,
    pub imported_at_unix_ms: i64,
    pub tags: Vec<String>,
    pub fidelity: Fidelity,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Complete,
    Partial,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct TargetInfo {
    /// `"go"` or `"dotnet"`.
    pub runtime: String,
    pub pid: Option<u32>,
    pub command: Option<String>,
    pub url: Option<String>,
    pub runtime_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionRecord {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub target: TargetInfo,
    pub status: SessionStatus,
    pub started_at_unix_ms: i64,
    pub duration_ms: Option<i64>,
    pub requested_kinds: Vec<ProfileKind>,
    pub warnings: Vec<String>,
    pub tags: Vec<String>,
    pub profile_ids: Vec<String>,
}

pub struct Store {
    root: PathBuf,
    conn: Mutex<Connection>,
}

/// `PRISM_HOME` if set and non-empty, else the platform data dir joined with `prism`,
/// else `./.prism` when the needed environment variables are missing.
pub fn default_home() -> PathBuf {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
    if let Some(v) = env("PRISM_HOME") {
        return PathBuf::from(v);
    }
    let base = if cfg!(target_os = "windows") {
        env("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| PathBuf::from(h).join("Library").join("Application Support"))
    } else {
        env("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| env("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
    };
    match base {
        Some(b) => b.join("prism"),
        None => PathBuf::from(".prism"),
    }
}

const COLUMNS: &str = "id, short_id, name, kind, source_format, sample_types, \
    default_sample_type, sample_count, default_total, imported_at_unix_ms, tags, \
    fidelity, session_id";

fn unit_str(u: &Unit) -> String {
    match u {
        Unit::Nanoseconds => "nanoseconds".into(),
        Unit::Bytes => "bytes".into(),
        Unit::Count => "count".into(),
        Unit::Other(s) => s.clone(),
    }
}

fn enum_str<T: Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(),
    }
}

fn enum_parse<T: serde::de::DeserializeOwned>(s: String) -> rusqlite::Result<T> {
    serde_json::from_value(serde_json::Value::String(s)).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn json_col<T: serde::de::DeserializeOwned>(row: &Row, idx: usize) -> rusqlite::Result<T> {
    let s: String = row.get(idx)?;
    serde_json::from_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn row_to_record(row: &Row) -> rusqlite::Result<ProfileRecord> {
    Ok(ProfileRecord {
        id: row.get(0)?,
        short_id: row.get(1)?,
        name: row.get(2)?,
        kind: enum_parse(row.get(3)?)?,
        source_format: enum_parse(row.get(4)?)?,
        sample_types: json_col(row, 5)?,
        default_sample_type: row.get::<_, i64>(6)? as usize,
        sample_count: row.get::<_, i64>(7)? as u64,
        default_total: row.get(8)?,
        imported_at_unix_ms: row.get(9)?,
        tags: json_col(row, 10)?,
        fidelity: enum_parse(row.get(11)?)?,
        session_id: row.get(12)?,
    })
}

const SESSION_COLUMNS: &str = "id, short_id, name, target, status, started_at_unix_ms, \
    duration_ms, requested_kinds, warnings, tags";

fn row_to_session(row: &Row) -> rusqlite::Result<SessionRecord> {
    Ok(SessionRecord {
        id: row.get(0)?,
        short_id: row.get(1)?,
        name: row.get(2)?,
        target: json_col(row, 3)?,
        status: enum_parse(row.get(4)?)?,
        started_at_unix_ms: row.get(5)?,
        duration_ms: row.get(6)?,
        requested_kinds: json_col(row, 7)?,
        warnings: json_col(row, 8)?,
        tags: json_col(row, 9)?,
        profile_ids: Vec::new(),
    })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn to_json<T: Serialize + ?Sized>(v: &T) -> Result<String, StoreError> {
    serde_json::to_string(v).map_err(|e| StoreError::Corrupt(e.to_string()))
}

fn default_name(file_name: Option<&str>, short_id: &str) -> String {
    if let Some(f) = file_name {
        let p = Path::new(f);
        if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
            let stem = stem.strip_suffix(".pb").unwrap_or(stem);
            if !stem.is_empty() {
                return stem.to_string();
            }
        }
    }
    short_id.to_string()
}

const SCHEMA_VERSION: i64 = 2;

const V1_DDL: &str = "CREATE TABLE IF NOT EXISTS profiles (
    id TEXT PRIMARY KEY,
    short_id TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    source_format TEXT NOT NULL,
    sample_types TEXT NOT NULL,
    default_sample_type INTEGER NOT NULL,
    sample_count INTEGER NOT NULL,
    default_total INTEGER NOT NULL,
    imported_at_unix_ms INTEGER NOT NULL,
    tags TEXT NOT NULL
);";

const V2_MIGRATION: &str =
    "ALTER TABLE profiles ADD COLUMN fidelity TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE profiles ADD COLUMN session_id TEXT NULL;
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    short_id TEXT NOT NULL,
    name TEXT NOT NULL,
    target TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at_unix_ms INTEGER NOT NULL,
    duration_ms INTEGER NULL,
    requested_kinds TEXT NOT NULL,
    warnings TEXT NOT NULL,
    tags TEXT NOT NULL
);";

impl Store {
    pub fn open(root: &Path) -> Result<Store, StoreError> {
        std::fs::create_dir_all(root)?;
        std::fs::create_dir_all(root.join("blobs"))?;
        let conn = Connection::open(root.join("prism.db"))?;
        conn.busy_timeout(Duration::from_secs(5))?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(StoreError::Corrupt(format!(
                "database schema version {version} is newer than supported {SCHEMA_VERSION}"
            )));
        }
        conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
        if version < SCHEMA_VERSION {
            let mut sql = String::from("BEGIN;");
            if version == 0 {
                sql.push_str(V1_DDL);
            }
            sql.push_str(V2_MIGRATION);
            sql.push_str("PRAGMA user_version = 2; COMMIT;");
            if let Err(e) = conn.execute_batch(&sql) {
                let _ = conn.execute_batch("ROLLBACK;");
                return Err(e.into());
            }
        }
        Ok(Store {
            root: root.to_path_buf(),
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn blob_path(&self, id: &str) -> PathBuf {
        self.root
            .join("blobs")
            .join(&id[..2])
            .join(format!("{id}.bin.zst"))
    }

    fn get(&self, id: &str) -> Result<Option<ProfileRecord>, StoreError> {
        let conn = self.conn();
        Ok(conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM profiles WHERE id = ?1"),
                params![id],
                row_to_record,
            )
            .optional()?)
    }

    pub fn import(
        &self,
        bytes: &[u8],
        opts: ImportOptions,
    ) -> Result<(ProfileRecord, bool), StoreError> {
        let id = hex::encode(Sha256::digest(bytes));
        if let Some(rec) = self.get(&id)? {
            if self.load(&id).is_err() {
                // Blob missing or undecodable: rewrite it from the new bytes.
                let mut profile = import_bytes(
                    bytes,
                    &ImportHints {
                        file_name: opts.file_name.as_deref(),
                        kind_override: opts.kind_override,
                    },
                )?;
                profile.meta.fidelity = rec.fidelity.clone();
                self.write_blob(&id, &profile, true)?;
            }
            return Ok((rec, false));
        }
        let mut profile = import_bytes(
            bytes,
            &ImportHints {
                file_name: opts.file_name.as_deref(),
                kind_override: opts.kind_override,
            },
        )?;
        if let Some(f) = opts.fidelity_override.clone() {
            profile.meta.fidelity = f;
        }
        self.write_blob(&id, &profile, false)?;

        let short_id = id[..12].to_string();
        let name = opts
            .name
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| default_name(opts.file_name.as_deref(), &short_id));
        let rec = ProfileRecord {
            short_id,
            name,
            kind: profile.meta.kind,
            source_format: profile.meta.source_format.clone(),
            sample_types: profile
                .sample_types
                .iter()
                .map(|s| SampleTypeInfo {
                    name: s.name.clone(),
                    unit: unit_str(&s.unit),
                })
                .collect(),
            default_sample_type: profile.default_sample_type,
            sample_count: profile.samples.len() as u64,
            default_total: profile.total(profile.default_sample_type),
            imported_at_unix_ms: now_ms(),
            tags: opts.tags,
            fidelity: profile.meta.fidelity.clone(),
            session_id: opts.session_id,
            id,
        };
        let conn = self.conn();
        let inserted = conn.execute(
            &format!(
                "INSERT OR IGNORE INTO profiles ({COLUMNS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"
            ),
            params![
                rec.id,
                rec.short_id,
                rec.name,
                enum_str(&rec.kind),
                enum_str(&rec.source_format),
                serde_json::to_string(&rec.sample_types)
                    .map_err(|e| StoreError::Corrupt(e.to_string()))?,
                rec.default_sample_type as i64,
                rec.sample_count as i64,
                rec.default_total,
                rec.imported_at_unix_ms,
                serde_json::to_string(&rec.tags).map_err(|e| StoreError::Corrupt(e.to_string()))?,
                enum_str(&rec.fidelity),
                rec.session_id,
            ],
        )?;
        if inserted == 0 {
            // Lost a race with a concurrent import of identical bytes.
            drop(conn);
            let existing = self
                .get(&rec.id)?
                .ok_or_else(|| StoreError::Corrupt("record vanished".into()))?;
            return Ok((existing, false));
        }
        Ok((rec, true))
    }

    fn write_blob(&self, id: &str, profile: &Profile, force: bool) -> Result<(), StoreError> {
        let path = self.blob_path(id);
        if !force && path.exists() {
            return Ok(());
        }
        let raw = bincode::serialize(profile).map_err(|e| StoreError::Corrupt(e.to_string()))?;
        let packed = zstd::encode_all(&raw[..], 3)?;
        let dir = path.parent().expect("blob path has parent");
        std::fs::create_dir_all(dir)?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let tmp = dir.join(format!("{id}.bin.zst.tmp-{}-{nanos}", std::process::id()));
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&packed)?;
            f.sync_all()?;
        }
        if let Err(e) = std::fs::rename(&tmp, &path) {
            let _ = std::fs::remove_file(&tmp);
            if force || !path.exists() {
                return Err(e.into());
            }
        }
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<ProfileRecord>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM profiles ORDER BY imported_at_unix_ms DESC, rowid DESC"
        ))?;
        let rows = stmt.query_map([], row_to_record)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn resolve(&self, reference: &str) -> Result<ProfileRecord, StoreError> {
        let not_found = || StoreError::NotFound(reference.to_string());
        if reference == "last" {
            return self.list()?.into_iter().next().ok_or_else(not_found);
        }
        if let Some(rec) = self.get(reference)? {
            return Ok(rec);
        }
        let all = self.list()?;
        if reference.len() >= 4 && reference.chars().all(|c| c.is_ascii_hexdigit()) {
            let lower = reference.to_ascii_lowercase();
            let mut hits: Vec<_> = all.iter().filter(|r| r.id.starts_with(&lower)).collect();
            match hits.len() {
                0 => {}
                1 => return Ok(hits.remove(0).clone()),
                _ => {
                    return Err(StoreError::Ambiguous {
                        reference: reference.to_string(),
                        matches: candidates(&hits),
                    })
                }
            }
        }
        let mut hits: Vec<_> = all.iter().filter(|r| r.name == reference).collect();
        match hits.len() {
            0 => Err(not_found()),
            1 => Ok(hits.remove(0).clone()),
            _ => Err(StoreError::Ambiguous {
                reference: reference.to_string(),
                matches: candidates(&hits),
            }),
        }
    }

    pub fn remove_profile(&self, id: &str) -> Result<(), StoreError> {
        let n = self
            .conn()
            .execute("DELETE FROM profiles WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(StoreError::NotFound(id.to_string()));
        }
        if id.len() >= 2 && id.is_ascii() {
            match std::fs::remove_file(self.blob_path(id)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    pub fn create_session(
        &self,
        name: &str,
        target: TargetInfo,
        kinds: &[ProfileKind],
        tags: &[String],
    ) -> Result<SessionRecord, StoreError> {
        let mut raw = [0u8; 16];
        getrandom::fill(&mut raw).map_err(|e| StoreError::Corrupt(format!("random: {e}")))?;
        let id = hex::encode(raw);
        let rec = SessionRecord {
            short_id: id[..12].to_string(),
            id,
            name: name.to_string(),
            target,
            status: SessionStatus::Running,
            started_at_unix_ms: now_ms(),
            duration_ms: None,
            requested_kinds: kinds.to_vec(),
            warnings: Vec::new(),
            tags: tags.to_vec(),
            profile_ids: Vec::new(),
        };
        self.conn().execute(
            &format!(
                "INSERT INTO sessions ({SESSION_COLUMNS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)"
            ),
            params![
                rec.id,
                rec.short_id,
                rec.name,
                to_json(&rec.target)?,
                enum_str(&rec.status),
                rec.started_at_unix_ms,
                rec.duration_ms,
                to_json(&rec.requested_kinds)?,
                to_json(&rec.warnings)?,
                to_json(&rec.tags)?,
            ],
        )?;
        Ok(rec)
    }

    pub fn finish_session(
        &self,
        id: &str,
        status: SessionStatus,
        duration_ms: i64,
        warnings: &[String],
    ) -> Result<SessionRecord, StoreError> {
        let n = self.conn().execute(
            "UPDATE sessions SET status = ?2, duration_ms = ?3, warnings = ?4 WHERE id = ?1",
            params![id, enum_str(&status), duration_ms, to_json(warnings)?],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound(id.to_string()));
        }
        self.get_session(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    fn fill_profile_ids(&self, mut s: SessionRecord) -> Result<SessionRecord, StoreError> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id FROM profiles WHERE session_id = ?1 ORDER BY imported_at_unix_ms, rowid",
        )?;
        s.profile_ids = stmt
            .query_map(params![s.id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(s)
    }

    fn get_session(&self, id: &str) -> Result<Option<SessionRecord>, StoreError> {
        let found = {
            let conn = self.conn();
            conn.query_row(
                &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?1"),
                params![id],
                row_to_session,
            )
            .optional()?
        };
        found.map(|s| self.fill_profile_ids(s)).transpose()
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionRecord>, StoreError> {
        let rows = {
            let conn = self.conn();
            let mut stmt = conn.prepare(&format!(
                "SELECT {SESSION_COLUMNS} FROM sessions ORDER BY started_at_unix_ms DESC, rowid DESC"
            ))?;
            let rows = stmt.query_map([], row_to_session)?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        rows.into_iter().map(|s| self.fill_profile_ids(s)).collect()
    }

    pub fn resolve_session(&self, reference: &str) -> Result<SessionRecord, StoreError> {
        let not_found = || StoreError::NotFound(reference.to_string());
        if reference == "last" {
            return self
                .list_sessions()?
                .into_iter()
                .next()
                .ok_or_else(not_found);
        }
        if let Some(s) = self.get_session(reference)? {
            return Ok(s);
        }
        let all = self.list_sessions()?;
        let cands = |h: &[&SessionRecord]| -> Vec<(String, i64)> {
            h.iter()
                .map(|r| (r.short_id.clone(), r.started_at_unix_ms))
                .collect()
        };
        if reference.len() >= 4 && reference.chars().all(|c| c.is_ascii_hexdigit()) {
            let lower = reference.to_ascii_lowercase();
            let mut hits: Vec<_> = all.iter().filter(|r| r.id.starts_with(&lower)).collect();
            match hits.len() {
                0 => {}
                1 => return Ok(hits.remove(0).clone()),
                _ => {
                    return Err(StoreError::Ambiguous {
                        reference: reference.to_string(),
                        matches: cands(&hits),
                    })
                }
            }
        }
        let mut hits: Vec<_> = all.iter().filter(|r| r.name == reference).collect();
        match hits.len() {
            0 => Err(not_found()),
            1 => Ok(hits.remove(0).clone()),
            _ => Err(StoreError::Ambiguous {
                reference: reference.to_string(),
                matches: cands(&hits),
            }),
        }
    }

    pub fn remove_session(&self, id: &str) -> Result<usize, StoreError> {
        let s = self
            .get_session(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))?;
        for pid in &s.profile_ids {
            self.remove_profile(pid)?;
        }
        self.conn()
            .execute("DELETE FROM sessions WHERE id = ?1", params![s.id])?;
        Ok(s.profile_ids.len())
    }

    pub fn load(&self, id: &str) -> Result<Profile, StoreError> {
        if id.len() < 2 || !id.is_ascii() {
            return Err(StoreError::NotFound(id.to_string()));
        }
        let path = self.blob_path(id);
        let packed = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(StoreError::Corrupt(format!("blob missing for {id}")))
            }
            Err(e) => return Err(e.into()),
        };
        let raw = zstd::decode_all(&packed[..])
            .map_err(|e| StoreError::Corrupt(format!("zstd decode of {id}: {e}")))?;
        bincode::deserialize(&raw)
            .map_err(|e| StoreError::Corrupt(format!("bincode decode of {id}: {e}")))
    }
}

fn candidates(hits: &[&ProfileRecord]) -> Vec<(String, i64)> {
    hits.iter()
        .map(|r| (r.short_id.clone(), r.imported_at_unix_ms))
        .collect()
}
