//! `prism check`: compare a profile with a baseline and fail on regressions.

use crate::format::format_value;
use prism_analysis::{
    diff_top_sorted, match_sample_type, DiffInput, DiffNormalize, DiffSort, Filter,
};
use prism_store::Store;
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Regression {
    name: String,
    base_self: i64,
    head_self: i64,
    /// `None` for a function that is new in the head.
    growth_pct: Option<f64>,
    /// Change in the function's share of its own profile, in percentage points.
    delta_points: f64,
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
    if let Some(n) = &a.sample_type {
        if base.sample_type_index(n).is_none() || head.sample_type_index(n).is_none() {
            return Err(format!("sample type '{n}' is not present in both profiles"));
        }
    }
    let (bi, hi) =
        match_sample_type(&base, &head, a.sample_type.as_deref()).map_err(|e| e.to_string())?;
    let (bt, ht) = (base.total(bi), head.total(hi));
    if bt <= 0 {
        return Err(format!(
            "the baseline '{}' has no samples to compare against",
            brec.name
        ));
    }
    if ht <= 0 {
        return Err(format!("the profile '{}' has no samples", hrec.name));
    }
    let rows = diff_top_sorted(
        DiffInput {
            profile: &base,
            sample_type: bi,
        },
        DiffInput {
            profile: &head,
            sample_type: hi,
        },
        &Filter::default(),
        DiffNormalize::None,
        usize::MAX,
        DiffSort::Delta,
    );
    let pct = |v: i64, total: i64| v as f64 / total as f64 * 100.0;
    let mut regressions: Vec<Regression> = rows
        .into_iter()
        .filter_map(|r| {
            let delta_points = pct(r.head_self, ht) - pct(r.base_self, bt);
            let growth = (r.base_self > 0)
                .then(|| (r.head_self - r.base_self) as f64 / r.base_self as f64 * 100.0);
            let regressed = match growth {
                Some(g) => g > a.threshold && delta_points >= a.min_value,
                None => r.head_self > 0 && pct(r.head_self, ht) >= a.min_value,
            };
            regressed.then_some(Regression {
                name: r.name,
                base_self: r.base_self,
                head_self: r.head_self,
                growth_pct: growth,
                delta_points,
            })
        })
        .collect();
    regressions.sort_by(|x, y| {
        y.delta_points
            .total_cmp(&x.delta_points)
            .then_with(|| x.name.cmp(&y.name))
    });
    let unit = brec
        .sample_types
        .get(bi)
        .map(|s| s.unit.clone())
        .unwrap_or_else(|| "count".into());
    let report = Report {
        ok: regressions.is_empty(),
        sample_type: SampleTypeOut {
            name: base.sample_types[bi].name.clone(),
            unit: unit.clone(),
        },
        base_total: bt,
        head_total: ht,
        regressions,
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
