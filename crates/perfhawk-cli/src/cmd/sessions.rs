use super::{format_duration_ms, kind_str, print_table};
use perfhawk_store::{SessionRecord, Store};

fn target_text(s: &SessionRecord) -> String {
    let t = &s.target;
    let what = if let Some(pid) = t.pid {
        format!("pid {pid}")
    } else if let Some(u) = &t.url {
        u.clone()
    } else {
        t.command.clone().unwrap_or_else(|| "-".into())
    };
    format!("{} {}", t.runtime, what)
}

pub fn run(store: &Store, json: bool) -> Result<(), String> {
    let sessions = store.list_sessions().map_err(|e| e.to_string())?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&sessions).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    let mut rows = vec![[
        "ID",
        "NAME",
        "STATUS",
        "STARTED (UTC)",
        "DURATION",
        "PROFILES",
        "TARGET",
    ]
    .map(String::from)
    .to_vec()];
    for s in &sessions {
        rows.push(vec![
            s.short_id.clone(),
            s.name.clone(),
            kind_str(&s.status),
            perfhawk_store::format_utc_minute(s.started_at_unix_ms),
            s.duration_ms
                .map(format_duration_ms)
                .unwrap_or_else(|| "-".into()),
            s.profile_ids.len().to_string(),
            target_text(s),
        ]);
    }
    print_table(&rows, &[4, 5]);
    Ok(())
}
