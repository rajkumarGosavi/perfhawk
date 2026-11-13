//! Runs Doctor over a recorded session. Shared by the HTTP API and `prism doctor`.

use prism_analysis::{diagnose, Diagnosis, DoctorConfig, DoctorInput, DoctorProfile};
use prism_store::{SessionRecord, Store, StoreError};

pub fn diagnose_session(store: &Store, session: &SessionRecord) -> Result<Diagnosis, StoreError> {
    let mut loaded = Vec::new();
    for id in &session.profile_ids {
        loaded.push((store.resolve(id)?, store.load(id)?));
    }
    let window_ns = session
        .duration_ms
        .filter(|d| *d > 0)
        .map(|d| d as u64 * 1_000_000)
        .or_else(|| loaded.iter().filter_map(|(_, p)| p.meta.duration_ns).max())
        .unwrap_or(0);
    let record_target = match (&session.target.pid, &session.target.url) {
        (Some(pid), _) => Some(format!("--pid {pid}")),
        (None, Some(url)) => Some(format!("--url {url}")),
        _ => None,
    };
    let input = DoctorInput {
        window_ns,
        profiles: loaded
            .iter()
            .map(|(rec, p)| DoctorProfile {
                kind: rec.kind,
                short_id: &rec.short_id,
                profile: p,
                fidelity: rec.fidelity.clone(),
            })
            .collect(),
        record_target,
    };
    Ok(diagnose(&input, &DoctorConfig::default()))
}
