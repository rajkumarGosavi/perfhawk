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
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("nettrace"))
        {
            failed |= import_nettrace(store, &path, &name, &tags);
            continue;
        }
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
                            ..Default::default()
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

/// `.nettrace` files go through the prism-dotnet helper, which yields one profile per kind.
fn import_nettrace(
    store: &Store,
    path: &std::path::Path,
    name: &Option<String>,
    tags: &[String],
) -> bool {
    let artifacts = match prism_capture::dotnet::convert_nettrace(path) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {}: {e}", path.display());
            return true;
        }
    };
    let base = name.clone().unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "nettrace".into())
    });
    let mut tags = tags.to_vec();
    tags.push("nettrace".into());
    let mut failed = false;
    for a in artifacts {
        let kind = a.kind;
        let res = store.import(
            &a.bytes,
            ImportOptions {
                name: Some(format!("{base}-{kind}")),
                file_name: Some(a.file_name),
                kind_override: Some(kind),
                tags: tags.clone(),
                fidelity_override: Some(a.fidelity),
                ..Default::default()
            },
        );
        match res {
            Ok((rec, fresh)) => println!(
                "{}  {}  {}  ({} samples){}",
                rec.short_id,
                super::kind_str(&rec.kind),
                rec.name,
                rec.sample_count,
                if fresh { "" } else { " [already imported]" }
            ),
            Err(e) => {
                eprintln!("error: {}: {kind}: {e}", path.display());
                failed = true;
            }
        }
    }
    failed
}
