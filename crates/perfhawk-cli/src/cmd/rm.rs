use perfhawk_store::Store;

/// Removes profiles (or sessions with `session`). Unknown references are
/// reported and skipped; returns the exit code.
pub fn run(store: &Store, refs: Vec<String>, session: bool) -> u8 {
    let mut failed = false;
    for r in refs {
        let result = if session {
            store.resolve_session(&r).and_then(|s| {
                store
                    .remove_session(&s.id)
                    .map(|_| (s.short_id.clone(), s.name.clone()))
            })
        } else {
            store.resolve(&r).and_then(|p| {
                store
                    .remove_profile(&p.id)
                    .map(|_| (p.short_id.clone(), p.name.clone()))
            })
        };
        match result {
            Ok((short, name)) => println!("removed {short} {name}"),
            Err(e) => {
                eprintln!("error: {r}: {e}");
                failed = true;
            }
        }
    }
    u8::from(failed)
}
