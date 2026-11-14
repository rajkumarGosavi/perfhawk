//! Discovery of processes that can be recorded. Shared by `perfhawk targets` and the UI.

use crate::go::discovery::{self, Discovery};
use crate::{dotnet, RuntimeInfo};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TargetRow {
    pub runtime: &'static str,
    pub pid: u32,
    pub version: Option<String>,
    pub agent: bool,
    pub command: Option<String>,
}

/// Pure mapping of discovery results to rows; a .NET entry without a pid is dropped.
pub fn rows_from(go: Vec<Discovery>, dotnet: Vec<RuntimeInfo>) -> Vec<TargetRow> {
    let mut rows: Vec<TargetRow> = go
        .into_iter()
        .map(|d| TargetRow {
            runtime: "go",
            pid: d.pid,
            version: d.go_version,
            agent: true,
            command: None,
        })
        .collect();
    rows.extend(dotnet.into_iter().filter_map(|p| {
        Some(TargetRow {
            runtime: "dotnet",
            pid: p.pid?,
            version: p.runtime_version,
            agent: false,
            command: p.command,
        })
    }));
    rows
}

/// Recordable processes, plus the error text when .NET discovery failed
/// (Go targets are still listed).
pub fn list_targets() -> (Vec<TargetRow>, Option<String>) {
    let (procs, warning) = match dotnet::list_dotnet_processes() {
        Ok(p) => (p, None),
        Err(e) => (Vec::new(), Some(e.to_string())),
    };
    (rows_from(discovery::list(), procs), warning)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::go::discovery::Discovery;
    use crate::RuntimeInfo;

    fn dotnet(pid: Option<u32>) -> RuntimeInfo {
        RuntimeInfo {
            provider: "dotnet",
            pid,
            url: None,
            runtime_version: Some("8.0.1".into()),
            command: Some("dotnet app.dll".into()),
        }
    }

    #[test]
    fn rows_from_maps_go_agent_and_dotnet_process() {
        let go = vec![Discovery {
            pid: 10,
            url: "http://127.0.0.1:6060".into(),
            go_version: Some("go1.22".into()),
        }];
        let rows = rows_from(go, vec![dotnet(Some(20)), dotnet(None)]);
        assert_eq!(rows.len(), 2, "a .NET entry without a pid is dropped");
        assert_eq!(
            (rows[0].runtime, rows[0].pid, rows[0].agent),
            ("go", 10, true)
        );
        assert_eq!(rows[0].command, None);
        assert_eq!(rows[0].version.as_deref(), Some("go1.22"));
        assert_eq!(
            (rows[1].runtime, rows[1].pid, rows[1].agent),
            ("dotnet", 20, false)
        );
        assert_eq!(rows[1].command.as_deref(), Some("dotnet app.dll"));
    }
}
