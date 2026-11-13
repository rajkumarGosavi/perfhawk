//! Series grouping and per-profile function history, for trend views.

use crate::filter::Filter;
use crate::pivot::function_stats;
use prism_model::Profile;
use serde::Serialize;

const SERIES_PREFIX: &str = "series:";
/// Launchers whose first argument names the real program.
const LAUNCHERS: [&str; 5] = ["dotnet", "java", "node", "python", "python3"];

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
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
        let mut tokens = cmd.split_whitespace();
        let first = tokens.next().unwrap_or(cmd);
        let program = if LAUNCHERS.contains(&basename(first)) {
            tokens.next().unwrap_or(first)
        } else {
            first
        };
        return format!("{runtime}:{}", basename(program));
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
