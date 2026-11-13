use clap::Parser;
use std::path::PathBuf;

/// Writes a large synthetic, uncompressed pprof profile.
#[derive(Parser)]
struct Args {
    /// Output file.
    #[arg(long)]
    out: PathBuf,
    /// Minimum output size in MiB.
    #[arg(long)]
    target_mb: u64,
    /// RNG seed; equal seeds give identical files.
    #[arg(long, default_value_t = 42)]
    seed: u64,
}

fn main() -> std::io::Result<()> {
    let a = Args::parse();
    genprof::generate(&a.out, a.target_mb, a.seed)
}
