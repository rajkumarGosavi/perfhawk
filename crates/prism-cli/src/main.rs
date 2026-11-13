//! `prism` command-line interface.

mod cmd;
mod format;

use clap::{CommandFactory, Parser, Subcommand};
use prism_model::ProfileKind;
use prism_store::{default_home, ImportOptions, Store};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "prism", version, about = "Local-first profiling workbench")]
struct Cli {
    /// Data directory (default: $PRISM_HOME or the platform data dir)
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
        /// Override the detected kind: cpu|wall|memory|lock|blocking|block_io|threads
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
    /// Serve the UI (profile list)
    Ui {
        #[arg(long, default_value_t = 7878)]
        port: u16,
        #[arg(long)]
        no_browser: bool,
    },
}

fn parse_kind(s: &str) -> Result<ProfileKind, String> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).map_err(|_| {
        format!("invalid kind '{s}' (expected cpu|wall|memory|lock|blocking|block_io|threads)")
    })
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
    let home = cli.home.unwrap_or_else(default_home);
    let store = match Store::open(&home) {
        Ok(s) => Arc::new(s),
        Err(e) => return fail(format!("cannot open store at {}: {e}", home.display())),
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
        } => done(cmd::show::run(
            &store,
            cmd::show::Args {
                reference,
                top,
                st,
                focus,
                exclude,
                hide,
            },
        )),
        Command::Open {
            target,
            port,
            no_browser,
        } => {
            let short = if Path::new(&target).exists() {
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
                Ok(id) => done(cmd::serve::run(store, port, Some(id), no_browser).await),
                Err(e) => fail(e),
            }
        }
        Command::Ui { port, no_browser } => {
            done(cmd::serve::run(store, port, None, no_browser).await)
        }
    }
}

fn done(r: Result<(), String>) -> ExitCode {
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(e),
    }
}
