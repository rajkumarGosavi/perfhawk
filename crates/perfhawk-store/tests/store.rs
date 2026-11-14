use perfhawk_import::{import_bytes, ImportHints};
use perfhawk_model::ProfileKind;
use perfhawk_store::{ImportOptions, Store, StoreError};

const CPU: &[u8] = include_bytes!("../../perfhawk-import/tests/fixtures/go-cpu.pb.gz");
const HEAP: &[u8] = include_bytes!("../../perfhawk-import/tests/fixtures/go-heap.pb.gz");
const FOLDED: &[u8] = include_bytes!("../../perfhawk-import/tests/fixtures/sample.folded");

fn opts(name: &str) -> ImportOptions {
    ImportOptions {
        name: Some(name.to_string()),
        ..Default::default()
    }
}

fn blob_count(root: &std::path::Path) -> usize {
    let mut n = 0;
    for d in std::fs::read_dir(root.join("blobs")).unwrap() {
        for f in std::fs::read_dir(d.unwrap().path()).unwrap() {
            if f.unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".bin.zst")
            {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn store_is_send_sync() {
    fn assert<T: Send + Sync>() {}
    assert::<Store>();
}

#[test]
fn import_then_load_roundtrip() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    let (rec, created) = store.import(CPU, ImportOptions::default()).unwrap();
    assert!(created);
    let loaded = store.load(&rec.id).unwrap();
    let fresh = import_bytes(CPU, &ImportHints::default()).unwrap();
    assert_eq!(loaded, fresh);
    assert_eq!(rec.sample_count, fresh.samples.len() as u64);
    assert_eq!(rec.default_total, fresh.total(fresh.default_sample_type));
    assert_eq!(rec.short_id, rec.id[..12]);
}

#[test]
fn reimport_same_bytes_dedupes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    let (a, c1) = store.import(CPU, ImportOptions::default()).unwrap();
    let (b, c2) = store.import(CPU, ImportOptions::default()).unwrap();
    assert!(c1 && !c2);
    assert_eq!(a.id, b.id);
    assert_eq!(store.list().unwrap().len(), 1);
    assert_eq!(blob_count(tmp.path()), 1);
}

#[test]
fn resolve_last_prefix_and_name() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    let (a, _) = store.import(CPU, opts("first")).unwrap();
    let (b, _) = store.import(HEAP, opts("second")).unwrap();
    assert_eq!(store.resolve("last").unwrap().id, b.id);
    assert_eq!(store.resolve(&a.id[..6]).unwrap().id, a.id);
    assert_eq!(store.resolve("first").unwrap().id, a.id);
    assert_eq!(store.resolve(&a.id).unwrap().id, a.id);
    assert_eq!(store.list().unwrap()[0].id, b.id);
}

#[test]
fn resolve_ambiguous_and_missing() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    let (a, _) = store.import(CPU, opts("svc")).unwrap();
    store.import(HEAP, opts("svc")).unwrap();
    let err = store.resolve("svc").unwrap_err();
    assert!(matches!(&err, StoreError::Ambiguous { matches, .. } if matches.len() == 2));
    let msg = err.to_string();
    assert!(msg.starts_with("'svc' is ambiguous (2 matches: "), "{msg}");
    assert!(msg.contains(&a.short_id), "{msg}");
    assert!(matches!(
        store.resolve(&a.id[..3]),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(
        store.resolve("nope"),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn resolve_last_on_empty_is_not_found() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    assert!(matches!(
        store.resolve("last"),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn default_name_from_file_name() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    let o = ImportOptions {
        file_name: Some("go-cpu.pb.gz".into()),
        ..Default::default()
    };
    assert_eq!(store.import(CPU, o).unwrap().0.name, "go-cpu");
    let (r, _) = store.import(HEAP, ImportOptions::default()).unwrap();
    assert_eq!(r.name, r.short_id);
}

#[test]
fn kind_override_and_tags_persist() {
    let tmp = tempfile::TempDir::new().unwrap();
    let id = {
        let store = Store::open(tmp.path()).unwrap();
        let o = ImportOptions {
            kind_override: Some(ProfileKind::Wall),
            tags: vec!["baseline".into()],
            ..Default::default()
        };
        store.import(FOLDED, o).unwrap().0.id
    };
    let store = Store::open(tmp.path()).unwrap();
    let rec = store.resolve(&id).unwrap();
    assert_eq!(rec.kind, ProfileKind::Wall);
    assert_eq!(rec.tags, vec!["baseline".to_string()]);
    assert!(!rec.sample_types.is_empty());
}

#[test]
fn store_path_with_spaces() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().join("my profiles");
    let store = Store::open(&root).unwrap();
    let (rec, _) = store.import(CPU, ImportOptions::default()).unwrap();
    store.load(&rec.id).unwrap();
}

#[test]
fn corrupt_blob_reports_corrupt() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path()).unwrap();
    let (rec, _) = store.import(CPU, ImportOptions::default()).unwrap();
    let blob = tmp
        .path()
        .join("blobs")
        .join(&rec.id[..2])
        .join(format!("{}.bin.zst", rec.id));
    std::fs::write(&blob, b"junk junk junk").unwrap();
    assert!(matches!(store.load(&rec.id), Err(StoreError::Corrupt(_))));
    std::fs::remove_file(&blob).unwrap();
    assert!(matches!(store.load(&rec.id), Err(StoreError::Corrupt(_))));
}

#[test]
fn newer_schema_version_is_rejected() {
    let tmp = tempfile::TempDir::new().unwrap();
    drop(Store::open(tmp.path()).unwrap());
    let raw = rusqlite::Connection::open(tmp.path().join("perfhawk.db")).unwrap();
    raw.execute_batch("PRAGMA user_version = 3;").unwrap();
    drop(raw);
    assert!(matches!(
        Store::open(tmp.path()),
        Err(StoreError::Corrupt(_))
    ));
}

#[test]
fn default_home_respects_perfhawk_home() {
    let old = std::env::var_os("PERFHAWK_HOME");
    std::env::set_var("PERFHAWK_HOME", "/tmp/custom perfhawk");
    assert_eq!(
        perfhawk_store::default_home(),
        std::path::PathBuf::from("/tmp/custom perfhawk")
    );
    std::env::set_var("PERFHAWK_HOME", "");
    assert!(
        perfhawk_store::default_home().ends_with("perfhawk")
            || perfhawk_store::default_home().ends_with(".perfhawk")
    );
    match old {
        Some(v) => std::env::set_var("PERFHAWK_HOME", v),
        None => std::env::remove_var("PERFHAWK_HOME"),
    }
}

mod pick_home {
    use perfhawk_store::{pick_home, HomeOs};
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| OsString::from(v))
        }
    }

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn windows_prefers_local_when_nothing_exists() {
        let e = env(&[("LOCALAPPDATA", "L"), ("APPDATA", "A")]);
        let got = pick_home(HomeOs::Windows, &e, &|_: &Path| false);
        assert_eq!(got, p("L").join("perfhawk"));
    }

    #[test]
    fn windows_keeps_existing_roaming_home() {
        let e = env(&[("LOCALAPPDATA", "L"), ("APPDATA", "A")]);
        let old = p("A").join("perfhawk");
        let got = pick_home(HomeOs::Windows, &e, &|x: &Path| x == old);
        assert_eq!(got, old);
    }

    #[test]
    fn windows_new_home_wins_when_both_exist() {
        let e = env(&[("LOCALAPPDATA", "L"), ("APPDATA", "A")]);
        let got = pick_home(HomeOs::Windows, &e, &|_: &Path| true);
        assert_eq!(got, p("L").join("perfhawk"));
    }

    #[test]
    fn windows_without_localappdata_uses_appdata() {
        let e = env(&[("APPDATA", "A")]);
        let got = pick_home(HomeOs::Windows, &e, &|_: &Path| false);
        assert_eq!(got, p("A").join("perfhawk"));
    }

    #[test]
    fn perfhawk_home_wins_everywhere() {
        let e = env(&[
            ("PERFHAWK_HOME", "/x"),
            ("LOCALAPPDATA", "L"),
            ("HOME", "/h"),
        ]);
        for os in [HomeOs::Windows, HomeOs::MacOs, HomeOs::Other] {
            assert_eq!(pick_home(os, &e, &|_: &Path| false), p("/x"));
        }
    }

    #[test]
    fn macos_and_other_keep_their_layout() {
        let e = env(&[("HOME", "/h")]);
        assert_eq!(
            pick_home(HomeOs::MacOs, &e, &|_: &Path| false),
            p("/h/Library/Application Support/perfhawk")
        );
        assert_eq!(
            pick_home(HomeOs::Other, &e, &|_: &Path| false),
            p("/h/.local/share/perfhawk")
        );
        let xdg = std::env::temp_dir().join("xdg");
        let xdg_s = xdg.to_string_lossy().into_owned();
        let pairs = [("XDG_DATA_HOME", xdg_s.as_str()), ("HOME", "/h")];
        let e = env(&pairs);
        assert_eq!(
            pick_home(HomeOs::Other, &e, &|_: &Path| false),
            xdg.join("perfhawk")
        );
        let e = env(&[]);
        assert_eq!(
            pick_home(HomeOs::Other, &e, &|_: &Path| false),
            p(".perfhawk")
        );
    }
}
