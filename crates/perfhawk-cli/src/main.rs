//! `perfhawk` command-line interface.

mod cmd;
mod format;

use clap::{CommandFactory, Parser, Subcommand};
use perfhawk_model::ProfileKind;
use perfhawk_store::{default_home, ImportOptions, Store};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "perfhawk", version, about = "Local-first profiling workbench")]
struct Cli {
    /// Data directory (default: $PERFHAWK_HOME or the platform data dir)
    #[arg(long, global = true, value_name = "DIR")]
    home: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Import profile files into the store
    Import {
        #[arg(required = true, value_name = "FILES")]
        files: Vec<PathBuf>,
        /// Display name (single file only)
        #[arg(long)]
        name: Option<String>,
        /// Override the detected kind: cpu|wall|memory|lock|blocking|block_io|off_cpu|sched_delay|threads
        #[arg(long, value_parser = parse_kind)]
        kind: Option<ProfileKind>,
        /// Tag to attach (repeatable)
        #[arg(long = "tag")]
        tags: Vec<String>,
    },
    /// List imported profiles
    Ls {
        #[arg(long)]
        json: bool,
    },
    /// Show the hottest functions of a profile
    Show {
        #[arg(default_value = "last")]
        reference: String,
        #[arg(long, default_value_t = 20)]
        top: usize,
        /// Sample type name
        #[arg(long)]
        st: Option<String>,
        #[arg(long, value_name = "RE")]
        focus: Option<String>,
        #[arg(long, value_name = "RE")]
        exclude: Option<String>,
        #[arg(long, value_name = "RE")]
        hide: Option<String>,
        /// Sort by self or total value
        #[arg(long, value_enum, default_value_t = SortArg::SelfValue)]
        sort: SortArg,
    },
    /// Serve the UI for a profile (an existing file is imported first)
    Open {
        #[arg(value_name = "REF_OR_FILE", default_value = "last")]
        target: String,
        #[arg(long, default_value_t = 7878)]
        port: u16,
        #[arg(long)]
        no_browser: bool,
    },
    /// Capture profiles from a live Go or .NET process
    Record {
        /// Attach to a process id
        #[arg(long)]
        pid: Option<u32>,
        /// Attach to a Go net/http/pprof endpoint
        #[arg(long, value_name = "URL")]
        url: Option<String>,
        /// Profile `go test` for a package (test arguments after `--`)
        #[arg(long, value_name = "PKG")]
        go_test: Option<String>,
        /// Kinds to capture, comma separated (default: everything available)
        #[arg(long, value_delimiter = ',', value_parser = parse_kind)]
        kinds: Vec<ProfileKind>,
        /// Capture window: <n>ms|s|m, 1s to 10m
        #[arg(long, default_value = "30s", value_parser = perfhawk_capture::parse_duration)]
        duration: Duration,
        /// Session name
        #[arg(long)]
        name: Option<String>,
        /// Tag to attach (repeatable)
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// Capture per-sample time for the heat map (the default where the runtime supports it)
        #[arg(long, conflicts_with = "no_timeline")]
        timeline: bool,
        /// Do not capture per-sample time
        #[arg(long)]
        no_timeline: bool,
        /// Open the UI on the new session afterwards
        #[arg(long)]
        open: bool,
        #[arg(long, default_value_t = 7878)]
        port: u16,
        /// Program to launch and its arguments
        #[arg(last = true, value_name = "PROGRAM")]
        program: Vec<String>,
    },
    /// Triage a service: what is slow (CPU, locks, I/O, scheduler) and what to run next
    Doctor {
        /// Attach to a process id
        #[arg(long)]
        pid: Option<u32>,
        /// Attach to a Go net/http/pprof endpoint
        #[arg(long, value_name = "URL")]
        url: Option<String>,
        /// Profile `go test` for a package (test arguments after `--`)
        #[arg(long, value_name = "PKG")]
        go_test: Option<String>,
        /// Capture window: <n>ms|s|m, 1s to 10m
        #[arg(long, default_value = "60s", value_parser = perfhawk_capture::parse_duration)]
        duration: Duration,
        /// Session name
        #[arg(long)]
        name: Option<String>,
        /// Diagnose an existing session instead of capturing
        #[arg(
            long,
            value_name = "REF",
            conflicts_with_all = ["pid", "url", "go_test", "check"]
        )]
        session: Option<String>,
        /// Machine-readable output
        #[arg(long)]
        json: bool,
        /// List what can be captured for the target and what is missing; capture nothing
        #[arg(long)]
        check: bool,
        /// After the report, run the recommended follow-up capture (never elevates)
        #[arg(long)]
        follow_up: bool,
        /// Program to launch and its arguments
        #[arg(last = true, value_name = "PROGRAM")]
        program: Vec<String>,
    },
    /// List recording targets (Go agents, .NET processes)
    Targets {
        #[arg(long)]
        json: bool,
    },
    /// List recording sessions
    Sessions {
        #[arg(long)]
        json: bool,
    },
    /// Compare two profiles
    Diff {
        base: String,
        head: String,
        /// Sample type name
        #[arg(long)]
        st: Option<String>,
        /// Scale the base to the head total before comparing
        #[arg(long, value_enum, default_value_t = NormalizeArg::None)]
        normalize: NormalizeArg,
        #[arg(long, default_value_t = 20)]
        top: usize,
        #[arg(long, value_name = "RE")]
        focus: Option<String>,
        #[arg(long, value_name = "RE")]
        exclude: Option<String>,
        #[arg(long, value_name = "RE")]
        hide: Option<String>,
    },
    /// Fail when a profile regresses against a baseline
    #[command(
        after_help = "A function regresses when its self value grows by more than --threshold percent AND its share of the profile grows by at least --min-value percentage points. A function that is new in the profile regresses when it holds at least --min-value points. Improvements never fail the check. Growth is scaled by each profile's recorded duration when both have one, so a shorter or longer run does not hide or invent a regression; shares (for --min-value) always come from each profile's own total.\n\nExit codes: 0 no regression, 1 regression found, 2 usage or data error (unknown profile, no common sample type, empty baseline, bad option)."
    )]
    Check {
        /// The profile to check
        head: String,
        /// The profile to compare against
        #[arg(long, value_name = "REF")]
        baseline: String,
        /// Sample type name (default: the common default)
        #[arg(long, value_name = "NAME")]
        sample_type: Option<String>,
        /// Growth in percent above which a function regresses
        #[arg(long, default_value = "10", value_parser = non_negative)]
        threshold: f64,
        /// Minimum growth of a function's share, in percentage points
        #[arg(long, default_value = "1.0", value_parser = non_negative)]
        min_value: f64,
        /// Regressions to print (all still count)
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Machine-readable output
        #[arg(long)]
        json: bool,
    },
    /// Delete profiles (or whole sessions with --session)
    Rm {
        #[arg(required = true, value_name = "REF")]
        refs: Vec<String>,
        #[arg(long)]
        session: bool,
    },
    /// Serve the UI (profile list)
    Ui {
        #[arg(long, default_value_t = 7878)]
        port: u16,
        /// Open the UI in the default browser (it is only printed by default)
        #[arg(long)]
        open: bool,
        /// Accepted for compatibility; the browser is not opened unless --open is given
        #[arg(long, hide = true)]
        no_browser: bool,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum NormalizeArg {
    None,
    Total,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum SortArg {
    #[value(name = "self")]
    SelfValue,
    Total,
}

fn parse_kind(s: &str) -> Result<ProfileKind, String> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).map_err(|_| {
        format!("invalid kind '{s}' (expected cpu|wall|memory|lock|blocking|block_io|off_cpu|sched_delay|threads)")
    })
}

fn non_negative(s: &str) -> Result<f64, String> {
    let v: f64 = s.parse().map_err(|_| format!("'{s}' is not a number"))?;
    if v.is_finite() && v >= 0.0 {
        Ok(v)
    } else {
        Err(format!("'{s}' must be zero or more"))
    }
}

fn fail(msg: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {msg}");
    ExitCode::from(1)
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Command::Import {
        files,
        name: Some(_),
        ..
    } = &cli.command
    {
        if files.len() > 1 {
            let _ = Cli::command()
                .error(
                    clap::error::ErrorKind::ArgumentConflict,
                    "--name can only be used with a single file",
                )
                .print();
            return ExitCode::from(2);
        }
    }
    if let Command::Record {
        pid,
        url,
        go_test,
        program,
        ..
    } = &cli.command
    {
        let probe = cmd::record::Args {
            pid: *pid,
            url: url.clone(),
            go_test: go_test.clone(),
            trailing: program.clone(),
            kinds: vec![],
            duration: Duration::from_secs(1),
            name: None,
            tags: vec![],
            timeline: true,
        };
        if let Err(m) = cmd::record::build_target(&probe) {
            let _ = Cli::command()
                .error(clap::error::ErrorKind::ArgumentConflict, m)
                .print();
            return ExitCode::from(2);
        }
    }
    if let Command::Doctor {
        pid,
        url,
        go_test,
        program,
        session,
        ..
    } = &cli.command
    {
        let probe = cmd::record::Args {
            pid: *pid,
            url: url.clone(),
            go_test: go_test.clone(),
            trailing: program.clone(),
            kinds: vec![],
            duration: Duration::from_secs(1),
            name: None,
            tags: vec![],
            timeline: true,
        };
        let bad = match (session, cmd::record::build_target(&probe)) {
            (Some(_), _) if !program.is_empty() => {
                Some("--session cannot be combined with a program".to_string())
            }
            (Some(_), _) => None,
            (None, Ok(_)) => None,
            (None, Err(m)) => Some(format!(
                "{m} (or --session <REF> to diagnose a recorded session)"
            )),
        };
        if let Some(m) = bad {
            let _ = Cli::command()
                .error(clap::error::ErrorKind::ArgumentConflict, m)
                .print();
            return ExitCode::from(2);
        }
    }
    let home = cli.home.unwrap_or_else(default_home);
    let is_check = matches!(cli.command, Command::Check { .. });
    let store = match Store::open(&home) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            // `check` reserves exit 1 for "regression found".
            eprintln!("error: cannot open store at {}: {e}", home.display());
            return ExitCode::from(if is_check { 2 } else { 1 });
        }
    };
    match cli.command {
        Command::Import {
            files,
            name,
            kind,
            tags,
        } => ExitCode::from(cmd::import::run(&store, files, name, kind, tags) as u8),
        Command::Ls { json } => done(cmd::ls::run(&store, json)),
        Command::Show {
            reference,
            top,
            st,
            focus,
            exclude,
            hide,
            sort,
        } => done(cmd::show::run(
            &store,
            cmd::show::Args {
                reference,
                top,
                st,
                focus,
                exclude,
                hide,
                sort: match sort {
                    SortArg::SelfValue => perfhawk_analysis::TopSort::SelfValue,
                    SortArg::Total => perfhawk_analysis::TopSort::Total,
                },
            },
        )),
        Command::Open {
            target,
            port,
            no_browser,
        } => {
            let short = if Path::new(&target).is_file() {
                std::fs::read(&target)
                    .map_err(|e| format!("{target}: {e}"))
                    .and_then(|bytes| {
                        store
                            .import(
                                &bytes,
                                ImportOptions {
                                    file_name: Path::new(&target)
                                        .file_name()
                                        .map(|n| n.to_string_lossy().into_owned()),
                                    ..Default::default()
                                },
                            )
                            .map_err(|e| format!("{target}: {e}"))
                    })
                    .map(|(rec, _)| rec.short_id)
            } else {
                store
                    .resolve(&target)
                    .map(|r| r.short_id)
                    .map_err(|e| e.to_string())
            };
            match short {
                Ok(id) => done(
                    cmd::serve::run(store, port, Some(format!("profile={id}")), no_browser).await,
                ),
                Err(e) => fail(e),
            }
        }
        Command::Ui {
            port,
            open,
            no_browser,
        } => done(cmd::serve::run(store, port, None, no_browser || !open).await),
        Command::Record {
            pid,
            url,
            go_test,
            kinds,
            duration,
            name,
            tags,
            timeline: _,
            no_timeline,
            open,
            port,
            program,
        } => {
            let args = cmd::record::Args {
                pid,
                url,
                go_test,
                trailing: program,
                kinds,
                duration,
                name,
                tags,
                timeline: !no_timeline,
            };
            let target = match cmd::record::build_target(&args) {
                Ok(t) => t,
                Err(m) => return fail(m),
            };
            let (code, short) = cmd::record::run(&store, args, target);
            if open && code == 0 {
                if let Some(id) = short {
                    return done(
                        cmd::serve::run(store, port, Some(format!("session={id}")), false).await,
                    );
                }
            }
            ExitCode::from(code)
        }
        Command::Doctor {
            pid,
            url,
            go_test,
            duration,
            name,
            session,
            json,
            check,
            follow_up,
            program,
        } => {
            let target = if session.is_some() {
                None
            } else {
                let args = cmd::record::Args {
                    pid,
                    url,
                    go_test,
                    trailing: program,
                    kinds: vec![],
                    duration,
                    name: name.clone(),
                    tags: vec![],
                    timeline: true,
                };
                match cmd::record::build_target(&args) {
                    Ok(t) => Some(t),
                    Err(m) => return fail(m),
                }
            };
            ExitCode::from(cmd::doctor::run(
                &store,
                cmd::doctor::Args {
                    target,
                    session,
                    duration,
                    name,
                    json,
                    check,
                    follow_up,
                },
            ))
        }
        Command::Targets { json } => done(cmd::targets::run(json)),
        Command::Sessions { json } => done(cmd::sessions::run(&store, json)),
        Command::Diff {
            base,
            head,
            st,
            normalize,
            top,
            focus,
            exclude,
            hide,
        } => done(cmd::diff::run(
            &store,
            cmd::diff::Args {
                base,
                head,
                st,
                normalize: match normalize {
                    NormalizeArg::None => perfhawk_analysis::DiffNormalize::None,
                    NormalizeArg::Total => perfhawk_analysis::DiffNormalize::Total,
                },
                top,
                focus,
                exclude,
                hide,
            },
        )),
        Command::Check {
            head,
            baseline,
            sample_type,
            threshold,
            min_value,
            limit,
            json,
        } => ExitCode::from(cmd::check::run(
            &store,
            cmd::check::Args {
                head,
                baseline,
                sample_type,
                threshold,
                min_value,
                limit,
                json,
            },
        )),
        Command::Rm { refs, session } => ExitCode::from(cmd::rm::run(&store, refs, session)),
    }
}

fn done(r: Result<(), String>) -> ExitCode {
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(e),
    }
}
