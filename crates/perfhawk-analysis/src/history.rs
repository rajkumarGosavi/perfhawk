//! Series grouping and per-profile function history, for trend views.

use crate::filter::Filter;
use crate::pivot::function_stats;
use perfhawk_model::Profile;
use serde::Serialize;

const SERIES_PREFIX: &str = "series:";
/// Launchers whose first non-flag argument names the real program.
const LAUNCHERS: [&str; 6] = ["dotnet", "java", "node", "python", "python3", "go"];
/// Subcommands that sit between a launcher and the program it runs.
const SUBCOMMANDS: [&str; 7] = ["run", "test", "build", "bench", "vet", "install", "exec"];

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// `name` without a trailing `.exe` (any case).
fn strip_exe(name: &str) -> &str {
    if name.len() > 4 && name[name.len() - 4..].eq_ignore_ascii_case(".exe") {
        &name[..name.len() - 4]
    } else {
        name
    }
}

/// Splits a command line on whitespace, keeping double-quoted parts together.
fn tokens(cmd: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in cmd.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(cur);
    }
    out
}

/// The series a session belongs to: a `series:<name>` tag, else the program
/// (or URL) it profiled, else just its runtime.
pub fn series_key(
    tags: &[String],
    runtime: &str,
    command: Option<&str>,
    url: Option<&str>,
) -> String {
    if let Some(name) = tags
        .iter()
        .filter_map(|t| t.strip_prefix(SERIES_PREFIX))
        .find(|n| !n.is_empty())
    {
        return name.to_string();
    }
    if let Some(cmd) = command.map(str::trim).filter(|c| !c.is_empty()) {
        let toks = tokens(cmd);
        if let Some(first) = toks.first() {
            let launcher = strip_exe(basename(first));
            let program = if LAUNCHERS.contains(&launcher.to_ascii_lowercase().as_str()) {
                toks[1..]
                    .iter()
                    .find(|t| !t.starts_with('-') && !SUBCOMMANDS.contains(&t.as_str()))
                    .map(|t| strip_exe(basename(t)))
                    .unwrap_or(launcher)
            } else {
                launcher
            };
            return format!("{runtime}:{program}");
        }
    }
    if let Some(u) = url.filter(|u| !u.is_empty()) {
        return format!("{runtime}:{u}");
    }
    runtime.to_string()
}

/// One function's numbers in one profile of a series.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryValue {
    pub found: bool,
    pub value: i64,
    pub total: i64,
    /// Self value as a percentage of the profile total.
    pub pct: f64,
    /// Why the profile could not answer (for example a missing sample type).
    pub skipped: Option<String>,
}

pub fn history_point(
    p: &Profile,
    sample_type_name: &str,
    filter: &Filter,
    func_name: &str,
) -> HistoryValue {
    let Some(st) = p.sample_type_index(sample_type_name) else {
        return HistoryValue {
            found: false,
            value: 0,
            total: 0,
            pct: 0.0,
            skipped: Some(format!("sample type '{sample_type_name}' not present")),
        };
    };
    match function_stats(p, st, filter, func_name) {
        Some(r) => HistoryValue {
            found: true,
            value: r.self_value,
            total: r.total,
            pct: if r.self_pct.is_finite() {
                r.self_pct
            } else {
                0.0
            },
            skipped: None,
        },
        None => HistoryValue {
            found: false,
            value: 0,
            total: 0,
            pct: 0.0,
            skipped: None,
        },
    }
}
