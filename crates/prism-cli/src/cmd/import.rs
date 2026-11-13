use prism_model::ProfileKind;
use prism_store::{ImportOptions, Store};
use std::path::PathBuf;

pub fn run(
    store: &Store,
    files: Vec<PathBuf>,
    name: Option<String>,
    kind: Option<ProfileKind>,
    tags: Vec<String>,
) -> i32 {
    let mut failed = false;
    for path in files {
        let result = std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| {
                store
                    .import(
                        &bytes,
                        ImportOptions {
                            name: name.clone(),
                            file_name: path.file_name().map(|n| n.to_string_lossy().into_owned()),
                            kind_override: kind,
                            tags: tags.clone(),
                        },
                    )
                    .map_err(|e| e.to_string())
            });
        match result {
            Ok((rec, fresh)) => println!(
                "{}  {}  {}  ({} samples){}",
                rec.short_id,
                super::kind_str(&rec.kind),
                rec.name,
                rec.sample_count,
                if fresh { "" } else { " [already imported]" }
            ),
            Err(e) => {
                eprintln!("error: {}: {e}", path.display());
                failed = true;
            }
        }
    }
    i32::from(failed)
}
