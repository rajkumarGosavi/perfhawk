//! `perfhawk counters`: runtime counters (threadpool, goroutines, ...) for a session.

use super::print_table;
use perfhawk_analysis::counters::{summarize, CounterReport};
use perfhawk_model::CounterData;
use perfhawk_store::Store;

pub const NO_COUNTERS: &str =
    "session has no runtime counters; record again with perfhawk 0.2+ (Go agent v0.1.3+)";

fn num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.0}")
    } else {
        format!("{v:.2}")
    }
}

/// One line for the end of `perfhawk record`.
pub fn headline(d: &CounterData) -> String {
    let r = summarize(d);
    let mut out = format!("Counters: {} series", r.stats.len());
    for f in &r.findings {
        out.push_str(&format!("; {}", f.title));
    }
    out
}

pub fn run(store: &Store, reference: &str, json: bool) -> Result<(), String> {
    let session = store
        .resolve_session(reference)
        .map_err(|e| e.to_string())?;
    let data = store
        .load_counters(&session.id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| NO_COUNTERS.to_string())?;
    let report = summarize(&data);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    print_report(&session.name, &session.short_id, &data.runtime, &report);
    Ok(())
}

fn print_report(name: &str, short_id: &str, runtime: &str, r: &CounterReport) {
    println!("{name} ({short_id}) runtime counters, {runtime}");
    println!();
    let mut rows = vec![["counter", "unit", "min", "mean", "max", "last"]
        .map(String::from)
        .to_vec()];
    for s in &r.stats {
        rows.push(vec![
            s.name.clone(),
            s.unit.clone(),
            num(s.min),
            num(s.mean),
            num(s.max),
            num(s.last),
        ]);
    }
    print_table(&rows, &[2, 3, 4, 5]);
    println!();
    if r.findings.is_empty() {
        println!("No findings.");
        return;
    }
    println!("Findings:");
    for f in &r.findings {
        println!("- {}: {} (threshold: {})", f.title, f.detail, f.threshold);
    }
}
