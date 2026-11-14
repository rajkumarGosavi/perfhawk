//! `perfhawk doctor`: triage a service in about a minute, then say what to run next.

use super::{kind_str, print_table};
use perfhawk_analysis::{Diagnosis, NextStep};
use perfhawk_capture::{
    CancelToken, Capability, CapturePlan, ProviderRegistry, RecordRequest, RuntimeInfo, Target,
};
use perfhawk_model::ProfileKind;
use perfhawk_server::doctor::diagnose_session;
use perfhawk_store::{SessionRecord, Store};
use std::time::Duration;

/// Language kinds Doctor captures (memory profiles are not part of a triage).
const TRIAGE_KINDS: [ProfileKind; 5] = [
    ProfileKind::Cpu,
    ProfileKind::Wall,
    ProfileKind::Lock,
    ProfileKind::Blocking,
    ProfileKind::Threads,
];

pub struct Args {
    pub target: Option<Target>,
    pub session: Option<String>,
    pub duration: Duration,
    pub name: Option<String>,
    pub json: bool,
    pub check: bool,
    pub follow_up: bool,
}

fn runtime_for(s: &SessionRecord) -> RuntimeInfo {
    RuntimeInfo {
        provider: match s.target.runtime.as_str() {
            "go" => "go",
            "dotnet" => "dotnet",
            _ => "unknown",
        },
        pid: s.target.pid,
        url: s.target.url.clone(),
        runtime_version: s.target.runtime_version.clone(),
        command: s.target.command.clone(),
    }
}

fn print_caps(caps: &[Capability]) {
    let mut rows = vec![["KIND", "AVAILABLE", "FIDELITY", "NOTE"]
        .map(String::from)
        .to_vec()];
    for c in caps {
        rows.push(vec![
            kind_str(&c.kind),
            if c.available { "yes" } else { "no" }.into(),
            kind_str(&c.fidelity),
            c.note.clone().unwrap_or_default(),
        ]);
    }
    print_table(&rows, &[]);
}

fn print_report(d: &Diagnosis) {
    println!("Verdict: {}", d.summary);
    for f in &d.findings {
        println!(
            "  {}  [{}, {:.0}%]",
            f.title,
            kind_str(&f.confidence),
            f.share_pct
        );
        for n in &f.next {
            match &n.command {
                Some(c) => println!("    next: {c}"),
                None => println!("    next: {}", n.label),
            }
        }
        for h in &f.hints {
            println!("    hint: {h}");
        }
    }
    for n in &d.notes {
        println!("note: {n}");
    }
}

fn first_follow_up(d: &Diagnosis) -> Option<&NextStep> {
    d.findings
        .iter()
        .flat_map(|f| f.next.iter())
        .find(|n| n.follow_up.is_some())
}

pub fn run(store: &Store, a: Args) -> u8 {
    let registry = ProviderRegistry::with_defaults();
    let fail = |m: String| -> u8 {
        eprintln!("error: {m}");
        1
    };

    // The session to diagnose: an existing one, or a fresh capture.
    let session: SessionRecord = if let Some(r) = &a.session {
        match store.resolve_session(r) {
            Ok(s) => s,
            Err(e) => return fail(e.to_string()),
        }
    } else {
        let target = a.target.clone().expect("target checked by the caller");
        let (provider, rt) = match registry.detect(&target) {
            Ok(x) => x,
            Err(e) => return fail(e.to_string()),
        };
        let lang = provider.capabilities(&rt);
        let sys = registry.system_capabilities(&rt);
        if a.check {
            let mut all = lang;
            all.extend(sys);
            print_caps(&all);
            return 0;
        }
        let mut kinds: Vec<ProfileKind> = TRIAGE_KINDS
            .into_iter()
            .filter(|k| lang.iter().any(|c| c.kind == *k && c.available))
            .collect();
        kinds.extend(sys.iter().filter(|c| c.available).map(|c| c.kind));
        if kinds.is_empty() {
            return fail("nothing to capture: no profile kind is available for this target".into());
        }
        let missing: Vec<&Capability> = sys.iter().filter(|c| !c.available).collect();
        if let Some(c) = missing.first() {
            eprintln!(
                "note: OS-level data (block I/O, off-CPU, scheduler delay) will not be captured: {}",
                c.note.as_deref().unwrap_or("not available")
            );
        }
        let req = RecordRequest {
            target,
            plan: CapturePlan {
                kinds,
                duration: a.duration,
                timeline: true,
            },
            name: a.name.clone(),
            tags: vec![],
        };
        let cancel = CancelToken::install_ctrlc();
        match perfhawk_capture::record(store, &registry, &req, &cancel, &|m| eprintln!("{m}")) {
            Ok(s) => {
                for w in &s.warnings {
                    eprintln!("warning: {w}");
                }
                s
            }
            Err(e) => return fail(e.to_string()),
        }
    };

    let d = match diagnose_session(store, &session) {
        Ok(d) => d,
        Err(e) => return fail(e.to_string()),
    };
    if a.json {
        match serde_json::to_string_pretty(&d) {
            Ok(s) => println!("{s}"),
            Err(e) => return fail(e.to_string()),
        }
    } else {
        print_report(&d);
    }
    if !a.follow_up {
        return 0;
    }

    let Some(step) = first_follow_up(&d) else {
        println!("no follow-up capture to run");
        return 0;
    };
    let fu = step.follow_up.as_ref().expect("filtered above");
    let rt = runtime_for(&session);
    let sys = registry.system_capabilities(&rt);
    let blocked = fu.kinds.iter().find_map(|k| {
        sys.iter()
            .find(|c| c.kind == *k && !c.available)
            .map(|c| c.note.clone().unwrap_or_else(|| "not available".into()))
    });
    if let Some(why) = blocked {
        eprintln!("error: cannot run the follow-up: {why}");
        if let Some(c) = &step.command {
            eprintln!("run it yourself from an elevated shell (administrator on Windows, root on Linux): {c}");
        }
        return 1;
    }
    let target = match (session.target.pid, &session.target.url) {
        (Some(pid), _) => Target::Pid(pid),
        (None, Some(u)) => Target::Url(u.clone()),
        _ => return fail("the session has no pid or url to record again".into()),
    };
    let req = RecordRequest {
        target,
        plan: CapturePlan {
            kinds: fu.kinds.clone(),
            duration: Duration::from_secs(fu.duration_secs),
            timeline: true,
        },
        name: None,
        tags: vec![format!("doctor-follow-up-of:{}", session.short_id)],
    };
    let cancel = CancelToken::install_ctrlc();
    match perfhawk_capture::record(store, &registry, &req, &cancel, &|m| eprintln!("{m}")) {
        Ok(s) => {
            println!(
                "follow-up session {}  ({})",
                s.short_id,
                kind_str(&s.status)
            );
            0
        }
        Err(e) => fail(e.to_string()),
    }
}
