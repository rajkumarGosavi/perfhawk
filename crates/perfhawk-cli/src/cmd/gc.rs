//! `perfhawk gc`: GC counts, pauses, heap and allocation for a session.

use super::{format_duration_ms, kind_str, print_table};
use crate::format::format_value;
use perfhawk_analysis::gc::{summarize, GcConfig, GcReport};
use perfhawk_model::GcData;
use perfhawk_store::Store;

fn ns(v: u64) -> String {
    format_value(v as f64, "nanoseconds")
}

fn bytes(v: u64) -> String {
    format_value(v as f64, "bytes")
}

fn gen_label(g: Option<u8>) -> String {
    match g {
        Some(n) => format!("gen{n}"),
        None => "cycle".into(),
    }
}

/// One line for the end of `perfhawk record`.
pub fn headline(d: &GcData) -> String {
    let s = summarize(d, &GcConfig::default());
    if s.total_gcs == 0 {
        return "GC: no collections".into();
    }
    let gens: Vec<String> = s
        .by_generation
        .iter()
        .map(|g| format!("{} {}", gen_label(g.generation), g.count))
        .collect();
    let mut out = format!(
        "GC: {} collections ({}), max pause {}",
        s.total_gcs,
        gens.join(", "),
        ns(s.pause.max_ns)
    );
    if let Some(p) = s.peak_heap_bytes {
        out.push_str(&format!(", peak heap {}", bytes(p)));
    }
    out
}

pub fn run(store: &Store, reference: &str, json: bool) -> Result<(), String> {
    let session = store
        .resolve_session(reference)
        .map_err(|e| e.to_string())?;
    let report = perfhawk_server::gc::report_for_session(store, &session)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            format!(
                "session {} has no GC data; record a .NET target or run perfhawk import --gc <file>",
                session.short_id
            )
        })?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    print_report(
        &session.name,
        &session.short_id,
        session.duration_ms,
        &report,
    );
    Ok(())
}

fn load_report(
    store: &Store,
    reference: &str,
) -> Result<(perfhawk_store::SessionRecord, GcReport), String> {
    let session = store
        .resolve_session(reference)
        .map_err(|e| e.to_string())?;
    let report = perfhawk_server::gc::report_for_session(store, &session)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            format!(
                "session {} has no GC data; record a .NET or Go target or run perfhawk import --gc <file>",
                session.short_id
            )
        })?;
    Ok((session, report))
}

/// One comparison row: a label, both values and the change from baseline to new run.
fn diff_row(label: &str, a: f64, b: f64, fmt: impl Fn(f64) -> String) -> Vec<String> {
    let delta = b - a;
    let sign = if delta > 0.0 {
        "+"
    } else if delta < 0.0 {
        "-"
    } else {
        ""
    };
    let pct = if a != 0.0 {
        format!(" ({sign}{:.0}%)", (delta / a * 100.0).abs())
    } else {
        String::new()
    };
    vec![
        label.to_string(),
        fmt(a),
        fmt(b),
        format!("{sign}{}{pct}", fmt(delta.abs())),
    ]
}

pub fn run_diff(store: &Store, a_ref: &str, b_ref: &str, json: bool) -> Result<(), String> {
    let (sa, ra) = load_report(store, a_ref)?;
    let (sb, rb) = load_report(store, b_ref)?;
    if json {
        let v = serde_json::json!({
            "baseline": { "session": sa.short_id, "name": sa.name, "report": ra },
            "new": { "session": sb.short_id, "name": sb.name, "report": rb },
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    println!(
        "GC diff: {} ({}) -> {} ({})",
        sa.name, sa.short_id, sb.name, sb.short_id
    );
    let (a, b) = (&ra.summary, &rb.summary);
    let n = |v: f64| ns(v as u64);
    let by = |v: f64| bytes(v as u64);
    let cnt = |v: f64| format!("{v:.0}");
    let one = |v: f64| format!("{v:.1}");
    let mut rows = vec![["METRIC", "BASELINE", "NEW", "CHANGE"]
        .map(String::from)
        .to_vec()];
    rows.push(diff_row(
        "collections",
        a.total_gcs as f64,
        b.total_gcs as f64,
        cnt,
    ));
    rows.push(diff_row(
        "GCs per minute",
        a.gcs_per_min,
        b.gcs_per_min,
        one,
    ));
    rows.push(diff_row(
        "pause p50",
        a.pause.p50_ns as f64,
        b.pause.p50_ns as f64,
        n,
    ));
    rows.push(diff_row(
        "pause p99",
        a.pause.p99_ns as f64,
        b.pause.p99_ns as f64,
        n,
    ));
    rows.push(diff_row(
        "pause max",
        a.pause.max_ns as f64,
        b.pause.max_ns as f64,
        n,
    ));
    rows.push(diff_row(
        "pause share %",
        a.pause_share_pct,
        b.pause_share_pct,
        one,
    ));
    rows.push(diff_row(
        "long pauses",
        a.bad_pause_count as f64,
        b.bad_pause_count as f64,
        cnt,
    ));
    if let (Some(x), Some(y)) = (a.peak_heap_bytes, b.peak_heap_bytes) {
        rows.push(diff_row("peak heap", x as f64, y as f64, by));
    }
    if let (Some(x), Some(y)) = (a.peak_heap_goal_bytes, b.peak_heap_goal_bytes) {
        rows.push(diff_row("peak heap goal", x as f64, y as f64, by));
    }
    if let (Some(x), Some(y)) = (a.gc_cpu_pct, b.gc_cpu_pct) {
        rows.push(diff_row("GC CPU %", x, y, one));
    }
    rows.push(diff_row(
        "alloc mean/s",
        a.alloc_mean_bytes_per_s,
        b.alloc_mean_bytes_per_s,
        by,
    ));
    print_table(&rows, &[1, 2, 3]);
    Ok(())
}

fn print_report(name: &str, short_id: &str, duration_ms: Option<i64>, r: &GcReport) {
    let s = &r.summary;
    println!(
        "GC report: {name} ({short_id})  runtime {}, {} captured",
        r.runtime,
        format_duration_ms(duration_ms.unwrap_or(0))
    );
    for n in &r.notes {
        println!("note: {n}");
    }
    println!();
    println!("Collections");
    if s.total_gcs == 0 {
        println!("  (no collections happened during this capture)");
        return;
    }
    let mut rows = vec![["GEN", "COUNT", "PAUSE TOTAL", "P50", "P95", "P99", "MAX"]
        .map(String::from)
        .to_vec()];
    let pause_row = |label: String, count: u64, p: &perfhawk_analysis::gc::PauseStats| {
        vec![
            label,
            count.to_string(),
            ns(p.total_ns),
            ns(p.p50_ns),
            ns(p.p95_ns),
            ns(p.p99_ns),
            ns(p.max_ns),
        ]
    };
    for g in &s.by_generation {
        rows.push(pause_row(gen_label(g.generation), g.count, &g.pause));
    }
    rows.push(pause_row("all".into(), s.total_gcs, &s.pause));
    print_table(&rows, &[1, 2, 3, 4, 5, 6]);

    println!();
    println!("Pause");
    println!(
        "  total {} = {:.1}% of wall time",
        ns(s.pause.total_ns),
        s.pause_share_pct
    );
    println!(
        "  {:.1} GCs/min, one every {}, {} long pause(s) ({:.1}%)",
        s.gcs_per_min,
        ns(s.mean_interval_ns),
        s.bad_pause_count,
        s.bad_pause_pct
    );
    if let Some(since) = s.since_last_gc_ns {
        println!("  last GC {} before the capture ended", ns(since));
    }
    if let Some(c) = s.gc_cpu_pct {
        println!("  GC used {c:.1}% of CPU time");
    }
    println!("  pause = time application threads were stopped (not the GC's own running time)");

    println!();
    println!("Heap");
    match s.peak_heap_bytes {
        Some(p) => {
            print!("  peak {}", bytes(p));
            if let Some(m) = s.min_post_gc_heap_bytes {
                print!(", smallest after a GC {}", bytes(m));
            }
            println!();
        }
        None => println!("  heap sizes are not available from this runtime"),
    }
    if let Some(g) = s.peak_heap_goal_bytes {
        println!("  peak heap goal {}", bytes(g));
    }
    if let Some(c) = &r.config {
        let mut parts = Vec::new();
        if let Some(g) = c.gogc {
            parts.push(if g < 0 {
                "GOGC off".to_string()
            } else {
                format!("GOGC {g}")
            });
        }
        if let Some(m) = c.mem_limit_bytes {
            parts.push(format!("GOMEMLIMIT {}", bytes(m)));
        }
        if !parts.is_empty() {
            println!("  settings: {}", parts.join(", "));
        }
    }
    if let (Some(first), Some(last)) = (
        s.post_gen2_heap_bytes.first(),
        s.post_gen2_heap_bytes.last(),
    ) {
        println!(
            "  after gen2 GCs: {} -> {} ({} GCs)",
            bytes(*first),
            bytes(*last),
            s.post_gen2_heap_bytes.len()
        );
    }

    println!();
    println!("Allocation");
    println!(
        "  total {}, mean {}/s, peak {}/s",
        bytes(s.alloc_total_bytes),
        bytes(s.alloc_mean_bytes_per_s as u64),
        bytes(s.alloc_peak_bytes_per_s as u64)
    );

    println!();
    println!("Reasons");
    for c in &s.by_reason {
        println!("  {} {}", c.name, c.count);
    }

    println!();
    println!("Findings");
    if r.findings.is_empty() {
        println!("  none");
    }
    for f in &r.findings {
        println!("  [{}] {}", kind_str(&f.severity), f.title);
        println!("         {}", f.detail);
    }

    println!();
    println!("Longest pauses");
    let mut pauses = r.series.pauses.clone();
    pauses.sort_by_key(|p| std::cmp::Reverse(p.pause_ns));
    let mut rows = vec![["ID", "AT", "GEN", "PAUSE"].map(String::from).to_vec()];
    for p in pauses.iter().take(10) {
        rows.push(vec![
            p.id.to_string(),
            ns(p.start_ns),
            gen_label(p.generation),
            ns(p.pause_ns),
        ]);
    }
    print_table(&rows, &[0, 1, 3]);
}

#[cfg(test)]
mod tests {
    use super::headline;
    use perfhawk_model::{AllocSeries, Fidelity, GcData, GcEvent, HeapPoint, GC_SCHEMA};

    fn ev(id: u32, generation: Option<u8>, pause_ms: u64) -> GcEvent {
        GcEvent {
            id,
            start_ns: id as u64,
            generation,
            kind: "blocking".into(),
            reason: "allocation".into(),
            reason_raw: None,
            induced: false,
            pause_ns: pause_ms * 1_000_000,
            duration_ns: pause_ms * 1_000_000,
            heap_before_bytes: None,
            heap_after_bytes: None,
            gen_after: None,
            promoted_bytes: None,
        }
    }

    fn data(events: Vec<GcEvent>, heap: Vec<HeapPoint>) -> GcData {
        GcData {
            schema: GC_SCHEMA,
            runtime: "dotnet".into(),
            start_unix_ns: 0,
            duration_ns: 1_000_000_000,
            fidelity: Fidelity::Native,
            events,
            heap,
            alloc: AllocSeries {
                bucket_ns: 1_000_000_000,
                bytes: vec![],
            },
            notes: vec![],
            config: None,
            cpu: None,
        }
    }

    #[test]
    fn headline_exact_format() {
        let mut evs = vec![];
        for i in 0..30 {
            evs.push(ev(i, Some(0), 1));
        }
        for i in 30..39 {
            evs.push(ev(i, Some(1), 2));
        }
        for i in 39..42 {
            evs.push(ev(i, Some(2), if i == 41 { 12 } else { 3 }));
        }
        let heap = vec![HeapPoint {
            t_ns: 1,
            total_bytes: 220_000_000,
            goal_bytes: None,
            gen0: None,
            gen1: None,
            gen2: None,
            loh: None,
            poh: None,
        }];
        assert_eq!(
            headline(&data(evs, heap)),
            "GC: 42 collections (gen0 30, gen1 9, gen2 3), max pause 12 ms, peak heap 210 MiB"
        );
    }

    #[test]
    fn headline_omits_unknown_heap_and_names_generationless_cycles() {
        assert_eq!(
            headline(&data(vec![ev(1, None, 5)], vec![])),
            "GC: 1 collections (cycle 1), max pause 5 ms"
        );
    }

    #[test]
    fn headline_zero_events() {
        assert_eq!(headline(&data(vec![], vec![])), "GC: no collections");
    }
}
