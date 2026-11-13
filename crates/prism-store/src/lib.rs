//! Local profile store: SQLite metadata plus zstd-compressed bincode blobs.

use prism_import::{import_bytes, ImportError, ImportHints};
use prism_model::{Profile, ProfileKind, SourceFormat, Unit};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("import failed: {0}")]
    Import(#[from] ImportError),
    #[error("no profile matches '{0}'")]
    NotFound(String),
    #[error("'{reference}' is ambiguous ({matches} matches)")]
    Ambiguous { reference: String, matches: usize },
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("corrupt store data: {0}")]
    Corrupt(String),
}

#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    pub name: Option<String>,
    pub file_name: Option<String>,
    pub kind_override: Option<ProfileKind>,
    pub tags: Vec<String>,
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
}

pub struct Store {
    root: PathBuf,
    conn: Mutex<Connection>,
}

/// `PRISM_HOME` if set and non-empty, else the platform data dir `/prism`, else `./.prism`.
pub fn default_home() -> PathBuf {
    if let Some(v) = std::env::var_os("PRISM_HOME") {
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    match dirs::data_dir() {
        Some(d) => d.join("prism"),
        None => PathBuf::from(".prism"),
    }
}

const COLUMNS: &str = "id, short_id, name, kind, source_format, sample_types, \
    default_sample_type, sample_count, default_total, imported_at_unix_ms, tags";

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
    })
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

impl Store {
    pub fn open(root: &Path) -> Result<Store, StoreError> {
        std::fs::create_dir_all(root)?;
        std::fs::create_dir_all(root.join("blobs"))?;
        let conn = Connection::open(root.join("prism.db"))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS profiles (
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
            );
            PRAGMA user_version = 1;",
        )?;
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
            return Ok((rec, false));
        }
        let profile = import_bytes(
            bytes,
            &ImportHints {
                file_name: opts.file_name.as_deref(),
                kind_override: opts.kind_override,
            },
        )?;
        self.write_blob(&id, &profile)?;

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
            imported_at_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
            tags: opts.tags,
            id,
        };
        let conn = self.conn();
        let inserted = conn.execute(
            &format!(
                "INSERT OR IGNORE INTO profiles ({COLUMNS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)"
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

    fn write_blob(&self, id: &str, profile: &Profile) -> Result<(), StoreError> {
        let path = self.blob_path(id);
        if path.exists() {
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
        std::fs::write(&tmp, &packed)?;
        if let Err(e) = std::fs::rename(&tmp, &path) {
            let _ = std::fs::remove_file(&tmp);
            if !path.exists() {
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
                n => {
                    return Err(StoreError::Ambiguous {
                        reference: reference.to_string(),
                        matches: n,
                    })
                }
            }
        }
        let mut hits: Vec<_> = all.iter().filter(|r| r.name == reference).collect();
        match hits.len() {
            0 => Err(not_found()),
            1 => Ok(hits.remove(0).clone()),
            n => Err(StoreError::Ambiguous {
                reference: reference.to_string(),
                matches: n,
            }),
        }
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
