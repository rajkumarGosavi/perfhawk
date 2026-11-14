//! Helpers shared by integration tests.
#![allow(dead_code)]
use std::path::PathBuf;

/// CoreLib of the highest installed 8.x runtime (falls back to any version).
pub fn find_corelib() -> Option<PathBuf> {
    let roots = [
        std::env::var_os("DOTNET_ROOT").map(PathBuf::from),
        Some(PathBuf::from(r"C:\Program Files\dotnet")),
        Some(PathBuf::from("/usr/share/dotnet")),
        Some(PathBuf::from("/usr/lib/dotnet")),
    ];
    for root in roots.into_iter().flatten() {
        let base = root.join("shared").join("Microsoft.NETCore.App");
        let Ok(rd) = std::fs::read_dir(&base) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        dirs.sort();
        dirs.sort_by_key(|d| {
            !d.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("8."))
        });
        for d in dirs {
            let p = d.join("System.Private.CoreLib.dll");
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}
