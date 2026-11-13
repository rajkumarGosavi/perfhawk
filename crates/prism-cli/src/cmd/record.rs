use super::{format_duration_ms, kind_str, print_table};
use crate::format::format_value;
use prism_capture::{CancelToken, CapturePlan, ProviderRegistry, RecordRequest, Target};
use prism_model::ProfileKind;
use prism_store::{SessionStatus, Store};
use std::time::Duration;

pub struct Args {
    pub pid: Option<u32>,
    pub url: Option<String>,
    pub go_test: Option<String>,
    /// Program and arguments after `--` (or the `go test` arguments with `--go-test`).
    pub trailing: Vec<String>,
    pub kinds: Vec<ProfileKind>,
    pub duration: Duration,
    pub name: Option<String>,
    pub tags: Vec<String>,
    pub timeline: bool,
}

pub const ONE_TARGET: &str = "choose one target: --pid, --url, --go-test or -- <program>";

pub fn build_target(a: &Args) -> Result<Target, String> {
    let program = a.go_test.is_none() && !a.trailing.is_empty();
    let n = [
        a.pid.is_some(),
        a.url.is_some(),
        a.go_test.is_some(),
        program,
    ]
    .iter()
    .filter(|b| **b)
    .count();
    if n != 1 {
        return Err(ONE_TARGET.into());
    }
    Ok(if let Some(p) = a.pid {
        Target::Pid(p)
    } else if let Some(u) = &a.url {
        Target::Url(u.clone())
    } else if let Some(pkg) = &a.go_test {
        Target::GoTest {
            package: pkg.clone(),
            args: a.trailing.clone(),
        }
    } else {
        Target::Launch {
            program: a.trailing[0].clone(),
            args: a.trailing[1..].to_vec(),
            cwd: None,
        }
    })
}

/// Records, prints the summary, and returns `(exit code, session short id)`.
pub fn run(store: &Store, a: Args, target: Target) -> (u8, Option<String>) {
    let registry = ProviderRegistry::with_defaults();
    let cancel = CancelToken::install_ctrlc();
    let req = RecordRequest {
        target,
        plan: CapturePlan {
            kinds: a.kinds,
            duration: a.duration,
            timeline: a.timeline,
        },
        name: a.name,
        tags: a.tags,
    };
    let session =
        match prism_capture::record(store, &registry, &req, &cancel, &|m| eprintln!("{m}")) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: {e}");
                return (1, None);
            }
        };
    for w in &session.warnings {
        eprintln!("warning: {w}");
    }
    println!(
        "session {}  {}  {}  {}",
        session.short_id,
        session.name,
        kind_str(&session.status),
        format_duration_ms(session.duration_ms.unwrap_or(0))
    );
    let mut rows = vec![["ID", "KIND", "FIDELITY", "SAMPLES", "TOTAL", "NOTE"]
        .map(String::from)
        .to_vec()];
    for id in &session.profile_ids {
        let Ok(r) = store.resolve(id) else { continue };
        let unit = r
            .sample_types
            .get(r.default_sample_type)
            .map(|s| s.unit.as_str())
            .unwrap_or("count");
        rows.push(vec![
            r.short_id.clone(),
            kind_str(&r.kind),
            kind_str(&r.fidelity),
            r.sample_count.to_string(),
            format_value(r.default_total as f64, unit),
            if r.sample_count == 0 {
                "empty".into()
            } else {
                String::new()
            },
        ]);
    }
    print_table(&rows, &[3, 4]);
    let code = match session.status {
        SessionStatus::Complete => 0,
        SessionStatus::Partial => {
            eprintln!("note: target exited before the window ended; session is partial");
            0
        }
        SessionStatus::Cancelled => 130,
        SessionStatus::Failed | SessionStatus::Running => 1,
    };
    (code, Some(session.short_id))
}
