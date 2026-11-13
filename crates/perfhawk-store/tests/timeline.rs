use perfhawk_model::Timeline;
use perfhawk_store::{ImportOptions, Store, StoreError};

const CPU: &[u8] = include_bytes!("../../perfhawk-import/tests/fixtures/go-cpu.pb.gz");

fn store() -> (tempfile::TempDir, Store) {
    let d = tempfile::TempDir::new().unwrap();
    let s = Store::open(d.path()).unwrap();
    (d, s)
}

fn tl(n: usize) -> Timeline {
    Timeline {
        bucket_ns: 20_000_000,
        start_unix_ns: 1_700_000_000_000_000_000,
        total_buckets: 0,
        sample_bucket: (0..n as u32).collect(),
    }
}

fn sidecars(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    for d in std::fs::read_dir(root.join("blobs")).unwrap() {
        for f in std::fs::read_dir(d.unwrap().path()).unwrap() {
            let p = f.unwrap().path();
            if p.to_string_lossy().ends_with(".timeline.zst") {
                out.push(p);
            }
        }
    }
    out
}

#[test]
fn save_and_load_roundtrip() {
    let (_d, s) = store();
    let (rec, _) = s.import(CPU, ImportOptions::default()).unwrap();
    let n = s.load(&rec.id).unwrap().samples.len();
    let t = tl(n);
    s.save_timeline(&rec.id, &t).unwrap();
    assert_eq!(s.load_timeline(&rec.id).unwrap(), Some(t));
}

#[test]
fn missing_timeline_and_unknown_id_are_none() {
    let (_d, s) = store();
    let (rec, _) = s.import(CPU, ImportOptions::default()).unwrap();
    assert_eq!(s.load_timeline(&rec.id).unwrap(), None);
    assert_eq!(s.load_timeline("ffffffffffff").unwrap(), None);
    assert_eq!(s.load_timeline("x").unwrap(), None);
}

#[test]
fn truncated_sidecar_loads_as_none() {
    let (d, s) = store();
    let (rec, _) = s.import(CPU, ImportOptions::default()).unwrap();
    let n = s.load(&rec.id).unwrap().samples.len();
    s.save_timeline(&rec.id, &tl(n)).unwrap();
    let p = sidecars(d.path()).pop().unwrap();
    let bytes = std::fs::read(&p).unwrap();
    std::fs::write(&p, &bytes[..bytes.len() / 2]).unwrap();
    assert_eq!(s.load_timeline(&rec.id).unwrap(), None);
}

#[test]
fn length_mismatch_is_rejected_on_save() {
    let (_d, s) = store();
    let (rec, _) = s.import(CPU, ImportOptions::default()).unwrap();
    let n = s.load(&rec.id).unwrap().samples.len();
    let err = s.save_timeline(&rec.id, &tl(n + 3)).unwrap_err();
    assert!(matches!(err, StoreError::InvalidTimeline(_)), "{err}");
    assert_eq!(s.load_timeline(&rec.id).unwrap(), None);
}

#[test]
fn length_mismatch_on_disk_loads_as_none() {
    let (d, s) = store();
    let (a, _) = s.import(CPU, ImportOptions::default()).unwrap();
    let n = s.load(&a.id).unwrap().samples.len();
    s.save_timeline(&a.id, &tl(n)).unwrap();
    // A sidecar copied from a profile with a different sample count must not be trusted.
    let p = sidecars(d.path()).pop().unwrap();
    let other = tl(n + 1);
    let raw = bincode::serialize(&other).unwrap();
    std::fs::write(&p, zstd::encode_all(&raw[..], 3).unwrap()).unwrap();
    assert_eq!(s.load_timeline(&a.id).unwrap(), None);
}

#[test]
fn remove_profile_removes_the_sidecar() {
    let (d, s) = store();
    let (rec, _) = s.import(CPU, ImportOptions::default()).unwrap();
    let n = s.load(&rec.id).unwrap().samples.len();
    s.save_timeline(&rec.id, &tl(n)).unwrap();
    assert_eq!(sidecars(d.path()).len(), 1);
    s.remove_profile(&rec.id).unwrap();
    assert!(sidecars(d.path()).is_empty());
}

#[test]
fn import_options_timeline_is_saved_and_reimport_keeps_it() {
    let (_d, s) = store();
    let probe = Store::open(tempfile::TempDir::new().unwrap().path()).unwrap();
    let (r, _) = probe.import(CPU, ImportOptions::default()).unwrap();
    let n = probe.load(&r.id).unwrap().samples.len();
    let t = tl(n);
    let (rec, created) = s
        .import(
            CPU,
            ImportOptions {
                timeline: Some(t.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(created);
    assert_eq!(s.load_timeline(&rec.id).unwrap(), Some(t.clone()));
    s.import(CPU, ImportOptions::default()).unwrap();
    assert_eq!(s.load_timeline(&rec.id).unwrap(), Some(t));
}

#[test]
fn import_rejects_a_wrong_length_timeline() {
    let (_d, s) = store();
    let err = s
        .import(
            CPU,
            ImportOptions {
                timeline: Some(tl(9999)),
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(matches!(err, StoreError::InvalidTimeline(_)), "{err}");
}
