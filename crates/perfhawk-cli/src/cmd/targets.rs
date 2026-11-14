use super::print_table;
use perfhawk_capture::list_targets;

pub fn run(json: bool) -> Result<(), String> {
    let (targets, warning) = list_targets();
    if let Some(e) = warning {
        eprintln!("warning: {e}; showing Go targets only");
    }
    if json {
        let v: Vec<_> = targets
            .iter()
            .map(|t| {
                serde_json::json!({
                    "runtime": t.runtime,
                    "pid": t.pid,
                    "version": t.version,
                    "agent": t.agent,
                    "command": t.command,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    let dash = |s: Option<String>| s.unwrap_or_else(|| "-".into());
    let mut rows = vec![["RUNTIME", "PID", "VERSION", "AGENT", "COMMAND"]
        .map(String::from)
        .to_vec()];
    for t in targets {
        rows.push(vec![
            t.runtime.into(),
            t.pid.to_string(),
            dash(t.version),
            if t.agent { "yes" } else { "-" }.into(),
            dash(t.command),
        ]);
    }
    print_table(&rows, &[1]);
    Ok(())
}
