//! Finding Go processes that run the prism agent, plus small helpers for
//! recognising Go executables.

use crate::cancel::CancelToken;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const PROTOCOL: u64 = 1;

/// A verified, reachable prism agent.
#[derive(Debug, Clone, PartialEq)]
pub struct Discovery {
    pub pid: u32,
    /// `http://127.0.0.1:PORT`
    pub url: String,
    pub go_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentInfo {
    pub pid: u32,
    pub go_version: Option<String>,
}

/// `<temp>/prism-agents`, the same place the Go agent writes to.
pub fn agents_dir() -> PathBuf {
    std::env::temp_dir().join("prism-agents")
}

/// GETs `<base>/debug/prism/info`. `Ok(None)` if the endpoint does not exist
/// or is not a prism agent; `Err` if the address cannot be reached.
pub fn agent_info(base: &str, timeout: Duration) -> Result<Option<AgentInfo>, String> {
    let url = format!("{}/debug/prism/info", base.trim_end_matches('/'));
    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    match agent.get(&url).call() {
        Ok(resp) => {
            let v: serde_json::Value = match resp.into_json() {
                Ok(v) => v,
                Err(_) => return Ok(None),
            };
            if v.get("protocol").and_then(|p| p.as_u64()) != Some(PROTOCOL) {
                return Ok(None);
            }
            let Some(pid) = v.get("pid").and_then(|p| p.as_u64()) else {
                return Ok(None);
            };
            Ok(Some(AgentInfo {
                pid: pid as u32,
                go_version: v
                    .get("go_version")
                    .and_then(|g| g.as_str())
                    .map(str::to_string),
            }))
        }
        Err(ureq::Error::Status(_, _)) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Reads `<dir>/<pid>.json` and verifies the agent answers and reports the
/// same pid. Stale files (dead process, reused pid or port) give `None`.
pub fn discover_in(dir: &Path, pid: u32) -> Option<Discovery> {
    let raw = std::fs::read(dir.join(format!("{pid}.json"))).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    if v.get("pid")?.as_u64()? != pid as u64 || v.get("protocol")?.as_u64()? != PROTOCOL {
        return None;
    }
    let url = format!("http://{}", v.get("addr")?.as_str()?);
    let info = agent_info(&url, Duration::from_secs(2)).ok()??;
    if info.pid != pid {
        return None;
    }
    Some(Discovery {
        pid,
        url,
        go_version: info.go_version.or_else(|| {
            v.get("go_version")
                .and_then(|g| g.as_str())
                .map(str::to_string)
        }),
    })
}

pub fn discover(pid: u32) -> Option<Discovery> {
    discover_in(&agents_dir(), pid)
}

/// Every verified agent in `dir`.
pub fn list_in(dir: &Path) -> Vec<Discovery> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut out: Vec<Discovery> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let pid: u32 = name.strip_suffix(".json")?.parse().ok()?;
            discover_in(dir, pid)
        })
        .collect();
    out.sort_by_key(|d| d.pid);
    out
}

pub fn list() -> Vec<Discovery> {
    list_in(&agents_dir())
}

/// Polls for a child's discovery file. `alive` is checked on each poll so a
/// child that already exited ends the wait early.
pub fn wait_for_discovery(
    pid: u32,
    timeout: Duration,
    cancel: &CancelToken,
    alive: &mut dyn FnMut() -> bool,
) -> Option<Discovery> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(d) = discover(pid) {
            return Some(d);
        }
        if !alive() || Instant::now() >= deadline || cancel.wait_timeout(Duration::from_millis(100))
        {
            return None;
        }
    }
}

const MAGIC: &[u8] = b"\xff Go buildinf:";
const MAX_SCAN: u64 = 256 * 1024 * 1024;

/// True if the file contains the Go build-info marker.
pub fn has_go_buildinfo(path: &Path) -> bool {
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = vec![0u8; 64 * 1024 + MAGIC.len()];
    let mut carry = 0usize;
    let mut scanned = 0u64;
    loop {
        let Ok(n) = f.read(&mut buf[carry..]) else {
            return false;
        };
        if n == 0 {
            return false;
        }
        let end = carry + n;
        if buf[..end].windows(MAGIC.len()).any(|w| w == MAGIC) {
            return true;
        }
        scanned += n as u64;
        if scanned > MAX_SCAN {
            return false;
        }
        let keep = (MAGIC.len() - 1).min(end);
        buf.copy_within(end - keep..end, 0);
        carry = keep;
    }
}

/// Resolves a program name against PATH (and PATHEXT on Windows); paths with
/// a separator are used as given.
pub fn which(program: &str) -> Option<PathBuf> {
    let p = Path::new(program);
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(|s| s.to_string())
            .collect()
    } else {
        vec![]
    };
    let probe = |base: PathBuf| -> Option<PathBuf> {
        if base.is_file() {
            return Some(base);
        }
        for e in &exts {
            let mut s = base.clone().into_os_string();
            s.push(e);
            let c = PathBuf::from(s);
            if c.is_file() {
                return Some(c);
            }
        }
        None
    };
    if p.components().count() > 1 || p.is_absolute() {
        return probe(p.to_path_buf());
    }
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|d| probe(d.join(program)))
}
