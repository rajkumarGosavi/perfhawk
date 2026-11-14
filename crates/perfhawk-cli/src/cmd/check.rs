//! `perfhawk check`: compare a profile with a baseline and fail on regressions.

use crate::format::format_value;
use perfhawk_analysis::{run_check, CheckInput, Regression};
use perfhawk_store::Store;
use serde::Serialize;

pub struct Args {
    pub head: String,
    pub baseline: String,
    pub sample_type: Option<String>,
    pub threshold: f64,
    pub min_value: f64,
    pub limit: usize,
    pub json: bool,
}

#[derive(Serialize)]
struct SampleTypeOut {
    name: String,
    unit: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    ok: bool,
    sample_type: SampleTypeOut,
    base_total: i64,
    head_total: i64,
    regressions: Vec<Regression>,
}

/// Exit code: 0 no regression, 1 regression, 2 usage or data error.
pub fn run(store: &Store, a: Args) -> u8 {
    match run_inner(store, &a) {
        Ok(code) => code,
        Err(msg) => {
            eprintln!("error: {msg}");
            2
        }
    }
}

fn run_inner(store: &Store, a: &Args) -> Result<u8, String> {
    let hrec = store.resolve(&a.head).map_err(|e| e.to_string())?;
    let brec = store.resolve(&a.baseline).map_err(|e| e.to_string())?;
    let head = store.load(&hrec.id).map_err(|e| e.to_string())?;
    let base = store.load(&brec.id).map_err(|e| e.to_string())?;
    let r = run_check(&CheckInput {
        base: &base,
        head: &head,
        base_name: &brec.name,
        head_name: &hrec.name,
        sample_type: a.sample_type.as_deref(),
        threshold: a.threshold,
        min_value: a.min_value,
    })?;
    let (bt, ht) = (r.base_total, r.head_total);
    let unit = brec
        .sample_types
        .get(r.base_index)
        .map(|s| s.unit.clone())
        .unwrap_or_else(|| "count".into());
    let report = Report {
        ok: r.ok,
        sample_type: SampleTypeOut {
            name: r.sample_type_name,
            unit: unit.clone(),
        },
        base_total: bt,
        head_total: ht,
        regressions: r.regressions,
    };
    if a.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
    } else {
        println!("check: {} vs {}", hrec.name, brec.name);
        println!(
            "sample type: {}  base total: {}  head total: {}",
            report.sample_type.name,
            format_value(bt as f64, &unit),
            format_value(ht as f64, &unit)
        );
        for r in report.regressions.iter().take(a.limit) {
            let growth = r
                .growth_pct
                .map_or("new".to_string(), |g| format!("{g:+.0}%"));
            println!(
                "{}  {} -> {}  {}  ({:+.1} pts of total)",
                r.name,
                format_value(r.base_self as f64, &unit),
                format_value(r.head_self as f64, &unit),
                growth,
                r.delta_points
            );
        }
        if report.regressions.len() > a.limit {
            println!("... and {} more", report.regressions.len() - a.limit);
        }
        if report.ok {
            println!("OK: no regressions");
        } else {
            println!("FAIL: {} regression(s)", report.regressions.len());
        }
    }
    Ok(if report.ok { 0 } else { 1 })
}
