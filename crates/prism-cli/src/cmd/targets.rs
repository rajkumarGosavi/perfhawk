use super::print_table;
use prism_capture::{dotnet, go::discovery};

struct Row {
    runtime: &'static str,
    pid: u32,
    version: Option<String>,
    agent: bool,
    command: Option<String>,
}

pub fn run(json: bool) -> Result<(), String> {
    let mut targets: Vec<Row> = discovery::list()
        .into_iter()
        .map(|d| Row {
            runtime: "go",
            pid: d.pid,
            version: d.go_version,
            agent: true,
            command: None,
        })
        .collect();
    match dotnet::list_dotnet_processes() {
        Ok(procs) => targets.extend(procs.into_iter().filter_map(|p| {
            Some(Row {
                runtime: "dotnet",
                pid: p.pid?,
                version: p.runtime_version,
                agent: false,
                command: p.command,
            })
        })),
        Err(e) => eprintln!("warning: {e}; showing Go targets only"),
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
