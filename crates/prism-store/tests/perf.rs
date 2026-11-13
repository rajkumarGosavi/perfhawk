//! Phase-0 performance gate on a 500 MiB synthetic pprof. Run explicitly:
//! `cargo test -p prism-store --release --test perf -- --ignored --nocapture`

use prism_analysis::{flame, top, Filter, TopSort};
use prism_store::{ImportOptions, Store};
use std::time::{Duration, Instant};

fn timed<T>(label: &str, f: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let out = f();
    let took = start.elapsed();
    println!("{label:<24} {:>8.3} s", took.as_secs_f64());
    (out, took)
}

#[test]
#[ignore = "perf gate: generates a 500 MiB profile; run in --release"]
fn perf_500mb_gate() {
    let tmp = tempfile::TempDir::new().unwrap();
    let file = tmp.path().join("big.pb");
    timed("generate", || genprof::generate(&file, 500, 42).unwrap());
    let bytes = std::fs::read(&file).unwrap();
    std::fs::remove_file(&file).unwrap();
    println!(
        "file size                {:>8.1} MiB",
        bytes.len() as f64 / (1 << 20) as f64
    );

    let store = Store::open(&tmp.path().join("home")).unwrap();
    let ((rec, created), import_t) = timed("import", || {
        store
            .import(
                &bytes,
                ImportOptions {
                    file_name: Some("big.pb".into()),
                    ..Default::default()
                },
            )
            .unwrap()
    });
    assert!(created);
    drop(bytes);
    let (p, load_t) = timed("load", || store.load(&rec.id).unwrap());
    println!(
        "samples {} stack nodes {} functions {}",
        p.samples.len(),
        p.stacks.len(),
        p.functions.len()
    );
    let st = p.default_sample_type;
    let filter = Filter::default();
    let (json, flame_t) = timed("flame(0.0005)+json", || {
        let tree = flame(&p, st, &filter, 0.0005);
        serde_json::to_vec(&tree).unwrap()
    });
    println!(
        "flame json               {:>8.2} MiB",
        json.len() as f64 / (1 << 20) as f64
    );
    let (rows, top_t) = timed("top(100)", || top(&p, st, &filter, 100, TopSort::SelfValue));
    assert_eq!(rows.len(), 100);

    assert!(
        import_t <= Duration::from_secs(20),
        "import took {import_t:?}"
    );
    assert!(load_t <= Duration::from_secs(5), "load took {load_t:?}");
    assert!(flame_t <= Duration::from_secs(1), "flame took {flame_t:?}");
    assert!(
        json.len() <= 5 * 1024 * 1024,
        "flame json {} bytes",
        json.len()
    );
    assert!(top_t <= Duration::from_secs(1), "top took {top_t:?}");
}
