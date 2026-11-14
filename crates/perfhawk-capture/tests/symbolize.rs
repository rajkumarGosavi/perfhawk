use perfhawk_capture::symbolize::*;
use std::path::{Path, PathBuf};
use std::process::Command;

fn go_available() -> bool {
    Command::new("go")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

const SRC: &str = "package main\n\nimport \"fmt\"\n\nfunc work(n int) int { return n * 2 }\n\nfunc main() { fmt.Println(work(21)) }\n";

fn build(dir: &Path, name: &str, flags: &[&str]) -> PathBuf {
    let src = dir.join("main.go");
    std::fs::write(&src, SRC).unwrap();
    let out = dir.join(name);
    let st = Command::new("go")
        .arg("build")
        .args(flags)
        .arg("-o")
        .arg(&out)
        .arg(&src)
        .status()
        .unwrap();
    assert!(st.success(), "go build failed");
    out
}

#[test]
fn frame_display_formats() {
    assert_eq!(Frame::Named("main.main".into()).display(), "main.main");
    assert_eq!(
        Frame::ModuleOffset {
            module: "kernel32.dll".into(),
            offset: 0x1a2b
        }
        .display(),
        "kernel32.dll+0x1a2b"
    );
    assert_eq!(Frame::Unknown(0xdead).display(), "[unknown 0xdead]");
}

#[test]
fn go_binary_symbols_resolve_main_main() {
    if !go_available() {
        eprintln!("skipped: go not found");
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let exe = build(d.path(), "app.exe", &[]);
    let syms = go_symbols(&exe).unwrap();
    let m = syms
        .iter()
        .find(|s| s.name == "main.main")
        .expect("main.main in symbols");
    let mut s = Symbolizer::new();
    s.add_image(0x1000_0000, 1 << 32, &exe);
    assert_eq!(
        s.resolve(0x1000_0000 + m.start + 1),
        Frame::Named("main.main".into())
    );
    assert_eq!(s.unresolved(), 0);
}

#[test]
fn stripped_go_binary_reports_stripped() {
    if !go_available() {
        eprintln!("skipped: go not found");
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let exe = build(d.path(), "stripped.exe", &["-ldflags=-s -w"]);
    assert!(matches!(go_symbols(&exe), Err(SymbolizeError::Stripped)));
}

#[test]
fn address_in_image_without_symbol_is_module_offset() {
    let mut s = Symbolizer::new();
    s.add_image(0x7ff0_0000, 0x1000, Path::new("C:/Windows/kernel32.dll"));
    assert_eq!(
        s.resolve(0x7ff0_0000 + 0x10),
        Frame::ModuleOffset {
            module: "kernel32.dll".into(),
            offset: 0x10
        }
    );
}

#[test]
fn address_outside_any_image_is_unknown() {
    let s = Symbolizer::new();
    assert_eq!(s.resolve(0x42), Frame::Unknown(0x42));
}

#[test]
fn unresolved_counter_counts_fallbacks_only() {
    let mut s = Symbolizer::new();
    s.add_image(0x1000, 0x100, Path::new("a.dll"));
    s.add_method(0x9000, 0x10, "N.C::M".into());
    s.resolve(0x9004); // named: not counted
    s.resolve(0x1010); // module+offset
    s.resolve(0x5); // unknown
    assert_eq!(s.unresolved(), 2);
}

#[test]
fn method_ranges_resolve_addresses_inside_and_not_outside() {
    let mut s = Symbolizer::new();
    s.add_method(0x1000, 0x40, "A.B::C".into());
    assert_eq!(s.resolve(0x103f), Frame::Named("A.B::C".into()));
    assert_eq!(s.resolve(0x1040), Frame::Unknown(0x1040));
}

#[test]
fn overlapping_reload_prefers_latest_method() {
    // tiered JIT recompiles the method at a new address; both stay resolvable
    let mut s = Symbolizer::new();
    s.add_method(0x1000, 0x40, "A::M tier0".into());
    s.add_method(0x2000, 0x40, "A::M tier1".into());
    assert_eq!(s.resolve(0x1010), Frame::Named("A::M tier0".into()));
    assert_eq!(s.resolve(0x2010), Frame::Named("A::M tier1".into()));
    // same start reloaded: the latest wins
    s.add_method(0x1000, 0x40, "A::M tier0b".into());
    assert_eq!(s.resolve(0x1010), Frame::Named("A::M tier0b".into()));
}

#[test]
fn method_name_formats_namespace_class_method() {
    assert_eq!(
        method_name("DotnetDemo.Program", "DiskWorker", "instance void ()"),
        "DotnetDemo.Program::DiskWorker"
    );
    assert_eq!(method_name("", "Main", ""), "Main");
}

#[test]
fn load_methods_feeds_method_load_events_only() {
    use perfhawk_capture::sysevents::{MethodInfo, RawEvent, RawKind};
    let mut m = RawEvent::new(RawKind::MethodLoad, 0, 1, 1);
    m.method = Some(MethodInfo {
        start: 0x5000,
        size: 0x20,
        name: "N.C::M".into(),
    });
    let other = RawEvent::new(RawKind::FileRead, 0, 1, 1);
    let mut s = Symbolizer::new();
    load_methods(&[other, m], &mut s);
    assert_eq!(s.resolve(0x5010), Frame::Named("N.C::M".into()));
}
