//! Test-only stand-in for `prism-dotnet`: replays a JSON-lines script.
//!
//! `FAKE_SCRIPT` names the script. Each line is printed as-is, except
//! directives: `{"type":"artifact","fixture":"<file>",...}` copies the fixture
//! to `<--out>/<kind>.pb.gz` and prints a normal artifact line;
//! `{"type":"wait_stop"}` blocks until `stop` (or EOF) arrives on stdin;
//! `{"type":"sleep","ms":N}` sleeps; `{"type":"exit","code":N}` exits at once.

use std::io::{BufRead, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out_dir = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1))
        .cloned();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let script = std::env::var("FAKE_SCRIPT")
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok());
    let Some(script) = script else {
        let _ = writeln!(
            out,
            r#"{{"type":"hello","protocol":1,"helper_version":"fake"}}"#
        );
        let _ = writeln!(out, r#"{{"type":"done","end":"complete"}}"#);
        return;
    };
    for line in script.lines().filter(|l| !l.trim().is_empty()) {
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                // Malformed lines are passed through verbatim on purpose.
                let _ = writeln!(out, "{line}");
                let _ = out.flush();
                continue;
            }
        };
        // A `"for":"<command>"` field restricts a line to one helper command.
        if let Some(only) = v.get("for").and_then(|f| f.as_str()) {
            if args.first().map(String::as_str) != Some(only) {
                continue;
            }
        }
        let line = &{
            let mut v2 = v.clone();
            if let Some(o) = v2.as_object_mut() {
                o.remove("for");
            }
            v2.to_string()
        };
        match v.get("type").and_then(|t| t.as_str()) {
            Some("wait_stop") => {
                let stdin = std::io::stdin();
                for l in stdin.lock().lines() {
                    if l.map(|l| l.trim() == "stop").unwrap_or(true) {
                        break;
                    }
                }
            }
            Some("sleep") => {
                let ms = v.get("ms").and_then(|m| m.as_u64()).unwrap_or(0);
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
            Some("exit") => {
                let code = v.get("code").and_then(|c| c.as_i64()).unwrap_or(0) as i32;
                let _ = out.flush();
                std::process::exit(code);
            }
            Some("artifact") if v.get("fixture").is_some() => {
                let kind = v.get("kind").and_then(|k| k.as_str()).unwrap_or("cpu");
                let fixture = v["fixture"].as_str().unwrap_or_default();
                let dir = out_dir.clone().unwrap_or_else(|| ".".into());
                let dest = std::path::Path::new(&dir).join(format!("{kind}.pb.gz"));
                let _ = std::fs::create_dir_all(&dir);
                if let Err(e) = std::fs::copy(fixture, &dest) {
                    let _ = writeln!(
                        out,
                        r#"{{"type":"error","code":"internal","message":"fake: cannot copy fixture {fixture}: {e}"}}"#
                    );
                    return;
                }
                let samples = v.get("samples").and_then(|s| s.as_u64()).unwrap_or(0);
                let evt = serde_json::json!({
                    "type": "artifact",
                    "kind": kind,
                    "path": dest.to_string_lossy(),
                    "samples": samples,
                });
                let _ = writeln!(out, "{evt}");
            }
            _ => {
                let _ = writeln!(out, "{line}");
            }
        }
        let _ = out.flush();
    }
}
