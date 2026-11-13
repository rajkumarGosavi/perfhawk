use prism_import::pprof::encode::encode_pprof;
use prism_import::{import_pprof, ImportHints};
use std::collections::BTreeSet;

const CPU: &[u8] = include_bytes!("fixtures/go-cpu.pb.gz");

fn names(p: &prism_model::Profile) -> BTreeSet<String> {
    p.functions
        .iter()
        .map(|f| p.str(f.name).to_string())
        .collect()
}

#[test]
fn round_trip_preserves_stacks_and_totals() {
    let a = import_pprof(CPU, &ImportHints::default()).unwrap();
    let bytes = encode_pprof(&a).unwrap();
    assert!(bytes.starts_with(&[0x1f, 0x8b]), "gzip output");
    let b = import_pprof(&bytes, &ImportHints::default()).unwrap();
    assert_eq!(a.sample_types, b.sample_types);
    for i in 0..a.sample_types.len() {
        assert_eq!(a.total(i), b.total(i), "sample type {i}");
    }
    assert_eq!(names(&a), names(&b));
    assert_eq!(a.default_sample_type, b.default_sample_type);
    assert_eq!(a.meta.duration_ns, b.meta.duration_ns);
}
