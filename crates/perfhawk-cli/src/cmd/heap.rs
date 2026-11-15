//! `perfhawk snapshot` and `perfhawk heap`: take a .NET heap snapshot, then
//! list the types that hold the most memory.

use super::print_table;
use crate::format::format_value;
use perfhawk_analysis::heap::summarize;
use perfhawk_capture::CancelToken;
use perfhawk_store::{SessionStatus, Store, TargetInfo};

fn bytes(v: u64) -> String {
    format_value(v as f64, "bytes")
}

/// Takes a heap snapshot of `pid` and stores it as a session.
pub fn snapshot(
    store: &Store,
    pid: u32,
    name: Option<String>,
    tags: Vec<String>,
    timeout_ms: u64,
) -> Result<(), String> {
    eprintln!(
        "Taking a heap snapshot of pid {pid}. This runs a full GC in the target and pauses it \
         until the heap walk finishes; avoid it on a busy production process."
    );
    let started = std::time::Instant::now();
    let cancel = CancelToken::install_ctrlc();
    let snap = perfhawk_capture::dotnet::capture_heap_snapshot(pid, timeout_ms, &cancel)
        .map_err(|e| e.to_string())?;
    let label = name.unwrap_or_else(|| format!("heap-{pid}"));
    let mut all_tags = tags;
    all_tags.push("heap".into());
    let rec = store
        .create_session(
            &label,
            TargetInfo {
                runtime: snap.runtime.clone(),
                pid: Some(pid),
                command: None,
                url: None,
                runtime_version: None,
            },
            &[],
            &all_tags,
        )
        .map_err(|e| e.to_string())?;
    if let Err(e) = store.save_heap(&rec.id, &snap) {
        let _ = store.remove_session(&rec.id);
        return Err(e.to_string());
    }
    let mut warnings = Vec::new();
    if !snap.complete {
        warnings.push(
            "the snapshot is incomplete: the runtime dropped events (large heap or timeout); \
             sizes and counts are lower bounds"
                .to_string(),
        );
    }
    store
        .finish_session(
            &rec.id,
            SessionStatus::Complete,
            started.elapsed().as_millis() as i64,
            &warnings,
        )
        .map_err(|e| e.to_string())?;
    println!(
        "{}  heap  {}  ({} objects, {})",
        rec.short_id,
        label,
        snap.node_count(),
        bytes(snap.total_bytes())
    );
    for w in &warnings {
        eprintln!("warning: {w}");
    }
    Ok(())
}

pub fn run(store: &Store, reference: &str, top: usize, json: bool) -> Result<(), String> {
    let session = store
        .resolve_session(reference)
        .map_err(|e| e.to_string())?;
    let snap = store
        .load_heap(&session.id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            format!(
                "session {} has no heap snapshot; take one with perfhawk snapshot --pid <pid>",
                session.short_id
            )
        })?;
    let s = summarize(&snap, top);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    println!(
        "Heap snapshot {}  {}  ({} objects, {} references, {})",
        session.short_id,
        session.name,
        s.objects,
        s.references,
        bytes(s.total_bytes)
    );
    if !s.complete {
        println!("note: incomplete snapshot; counts and sizes are lower bounds");
    }
    println!("size = shallow (the objects' own bytes, not what they reference)");
    println!();
    let mut rows = vec![["COUNT", "SHALLOW", "SHARE", "TYPE"]
        .map(String::from)
        .to_vec()];
    for t in &s.types {
        rows.push(vec![
            t.count.to_string(),
            bytes(t.shallow_bytes),
            format!("{:.1}%", t.share_pct),
            t.name.clone(),
        ]);
    }
    print_table(&rows, &[0, 1, 2]);
    if (s.types.len() as u64) < s.type_count {
        println!(
            "({} more types; use --top to show more)",
            s.type_count - s.types.len() as u64
        );
    }
    if !s.roots.is_empty() {
        println!();
        let kinds: Vec<String> = s
            .roots
            .iter()
            .map(|r| format!("{} {}", r.kind, r.count))
            .collect();
        println!("Roots: {}", kinds.join(", "));
    }
    Ok(())
}
