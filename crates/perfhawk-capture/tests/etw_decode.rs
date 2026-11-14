//! Decoder tests. The byte vectors are real payloads captured with the ETW
//! spike on Windows 11 (see docs/perf/etw-spike.md).
use perfhawk_capture::etwdecode::*;
use perfhawk_capture::sysevents::{RawEvent, RawKind};

const TARGET: u32 = 100;
const TID: u32 = 8584; // 0x2188, the TTid in the real FileIo samples

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn wide(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(|c| c.to_le_bytes())
        .collect()
}

fn rec(provider: u128, opcode: u8, pid: u32, tid: u32, qpc: i64, data: &[u8]) -> Record<'_> {
    Record {
        provider,
        opcode,
        version: 3,
        pid,
        tid,
        qpc,
        data,
    }
}

const FILE_READ: &str = "0000000000000000f8b0be8d8fd2ffff40c232948fd2ffff906b28388b88ffff88210000000001004300060000000000";
const FILE_WRITE: &str = "4800000000000000883b20978fd2ffff40d932948fd2ffff906b28388b88ffff8821000070ff0300000a060000000000";
const OP_END: &str = "883b20978fd2ffff020000000000000000000000";
const CSWITCH: &str = "bc0100009418000017080000170009026000000036e2000001000000";
const READY: &str = "bc010000010a0000";
const DISK_READ: &str = "0100000043000600003c000000000000003c3f391000000090399e418b88ffff10301f978fd2ffff730600000000000088210000";

fn names(events: &[RawEvent], kind: RawKind) -> Vec<&RawEvent> {
    events.iter().filter(|e| e.kind == kind).collect()
}

fn stack_record(ts: i64, tid: u32, frames: &[u64]) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend(ts.to_le_bytes());
    d.extend(7u32.to_le_bytes()); // stack process
    d.extend(tid.to_le_bytes());
    for f in frames {
        d.extend(f.to_le_bytes());
    }
    d
}

#[test]
fn file_read_sample_decodes() {
    let mut d = Decoder::new(TARGET, []);
    let data = hex(FILE_READ);
    d.feed(&rec(GUID_FILE_IO, 67, TARGET, TID, 1_000, &data));
    let ev = d.finish();
    let r = names(&ev, RawKind::FileRead);
    assert_eq!(r.len(), 1);
    assert_eq!(
        (r[0].pid, r[0].tid, r[0].bytes, r[0].qpc),
        (TARGET, TID, 65_536, 1_000)
    );
}

#[test]
fn file_write_sample_decodes() {
    let mut d = Decoder::new(TARGET, []);
    d.feed(&rec(GUID_FILE_IO, 68, TARGET, TID, 1_000, &hex(FILE_WRITE)));
    let ev = d.finish();
    let w = names(&ev, RawKind::FileWrite);
    assert_eq!((w.len(), w[0].bytes), (1, 262_000));
}

#[test]
fn file_events_of_other_processes_are_dropped() {
    let mut d = Decoder::new(TARGET, []);
    d.feed(&rec(
        GUID_FILE_IO,
        68,
        999,
        5,
        1_000,
        &hex(FILE_WRITE.replace("88210000", "05000000").as_str()),
    ));
    assert!(d.finish().is_empty());
}

#[test]
fn op_end_sets_duration_by_irp() {
    let mut d = Decoder::new(TARGET, []);
    d.feed(&rec(GUID_FILE_IO, 68, TARGET, TID, 1_000, &hex(FILE_WRITE)));
    d.feed(&rec(GUID_FILE_IO, 76, TARGET, TID, 21_000, &hex(OP_END)));
    let ev = d.finish();
    assert_eq!(names(&ev, RawKind::FileWrite)[0].duration_qpc, 20_000);
}

#[test]
fn file_name_resolves_by_file_key_even_when_it_arrives_last() {
    let mut d = Decoder::new(TARGET, []);
    d.feed(&rec(GUID_FILE_IO, 67, TARGET, TID, 1_000, &hex(FILE_READ)));
    // rundown name event: FileKey then a UTF-16 path
    let mut n = hex("906b28388b88ffff");
    n.extend(wide("\\Device\\HarddiskVolume3\\data\\x.db"));
    d.feed(&rec(GUID_FILE_IO, 36, 4, 4, 9_000, &n));
    let ev = d.finish();
    assert_eq!(
        names(&ev, RawKind::FileRead)[0].file.as_deref(),
        Some("\\Device\\HarddiskVolume3\\data\\x.db")
    );
}

#[test]
fn file_name_falls_back_to_create_event_by_file_object() {
    let mut d = Decoder::new(TARGET, []);
    // FileIo Create (64): Irp, FileObject, TTid, options, attrs, share, then path at 32
    let mut c = hex("883b20978fd2ffff40d932948fd2ffff88210000200000058000000005000000");
    c.extend(wide("\\Device\\HarddiskVolume3\\tmp\\y.bin"));
    d.feed(&rec(GUID_FILE_IO, 64, TARGET, TID, 500, &c));
    d.feed(&rec(GUID_FILE_IO, 68, TARGET, TID, 1_000, &hex(FILE_WRITE)));
    let ev = d.finish();
    assert_eq!(
        names(&ev, RawKind::FileWrite)[0].file.as_deref(),
        Some("\\Device\\HarddiskVolume3\\tmp\\y.bin")
    );
}

#[test]
fn unnamed_file_stays_none() {
    let mut d = Decoder::new(TARGET, []);
    d.feed(&rec(GUID_FILE_IO, 67, TARGET, TID, 1_000, &hex(FILE_READ)));
    assert_eq!(d.finish()[0].file, None);
}

#[test]
fn disk_read_sample_decodes_duration_bytes_and_issuer() {
    let mut d = Decoder::new(TARGET, [TID]);
    d.feed(&rec(GUID_DISK_IO, 10, 0, 0, 5_000, &hex(DISK_READ)));
    let ev = d.finish();
    let r = names(&ev, RawKind::DiskRead);
    assert_eq!(r.len(), 1);
    assert_eq!(
        (r[0].tid, r[0].bytes, r[0].duration_qpc, r[0].pid),
        (TID, 15_360, 0x673, TARGET)
    );
}

#[test]
fn disk_read_by_another_thread_is_dropped() {
    let mut d = Decoder::new(TARGET, [1]);
    d.feed(&rec(GUID_DISK_IO, 10, 0, 0, 5_000, &hex(DISK_READ)));
    assert!(d.finish().is_empty());
}

#[test]
fn context_switch_of_a_target_thread_decodes() {
    let mut d = Decoder::new(TARGET, [444]);
    d.feed(&rec(GUID_THREAD, 36, 0, 0, 3_000, &hex(CSWITCH)));
    let ev = d.finish();
    let c = names(&ev, RawKind::CSwitch);
    assert_eq!(c.len(), 1);
    assert_eq!(
        (c[0].tid, c[0].new_tid, c[0].old_tid, c[0].pid),
        (444, 444, 6292, TARGET)
    );
}

#[test]
fn switching_out_a_target_thread_is_kept_with_pid_zero() {
    let mut d = Decoder::new(TARGET, [6292]);
    d.feed(&rec(GUID_THREAD, 36, 0, 0, 3_000, &hex(CSWITCH)));
    let ev = d.finish();
    assert_eq!((ev.len(), ev[0].pid, ev[0].old_tid), (1, 0, 6292));
}

#[test]
fn context_switch_between_other_threads_is_dropped() {
    let mut d = Decoder::new(TARGET, [1, 2]);
    d.feed(&rec(GUID_THREAD, 36, 0, 0, 3_000, &hex(CSWITCH)));
    assert!(d.finish().is_empty());
}

#[test]
fn thread_start_of_the_target_adds_its_tid() {
    let mut d = Decoder::new(TARGET, []);
    // Thread start: ProcessId, TThreadId
    let mut t = Vec::new();
    t.extend(TARGET.to_le_bytes());
    t.extend(444u32.to_le_bytes());
    d.feed(&rec(GUID_THREAD, 1, TARGET, 1, 100, &t));
    d.feed(&rec(GUID_THREAD, 36, 0, 0, 3_000, &hex(CSWITCH)));
    assert_eq!(names(&d.finish(), RawKind::CSwitch).len(), 1);
}

#[test]
fn ready_thread_of_a_target_thread_decodes() {
    let mut d = Decoder::new(TARGET, [444]);
    d.feed(&rec(GUID_THREAD, 50, 55, 56, 4_000, &hex(READY)));
    let ev = d.finish();
    let r = names(&ev, RawKind::ReadyThread);
    assert_eq!(
        (r.len(), r[0].tid, r[0].new_tid, r[0].pid),
        (1, 444, 444, TARGET)
    );
}

#[test]
fn stack_arriving_after_the_event_is_attached_user_frames_only() {
    let mut d = Decoder::new(TARGET, []);
    d.feed(&rec(GUID_FILE_IO, 67, TARGET, TID, 1_000, &hex(FILE_READ)));
    let s = stack_record(
        1_000,
        TID,
        &[0xffff_f807_eb00_0000, 0x7ff6_1000, 0x7ff6_2000],
    );
    d.feed(&rec(GUID_STACK_WALK, 32, 0, 0, 1_050, &s));
    assert_eq!(d.finish()[0].stack, vec![0x7ff6_1000, 0x7ff6_2000]);
}

#[test]
fn stack_arriving_before_the_event_is_attached() {
    let mut d = Decoder::new(TARGET, [TID]);
    let s = stack_record(1_000, TID, &[0x7ff6_1000]);
    d.feed(&rec(GUID_STACK_WALK, 32, 0, 0, 900, &s));
    d.feed(&rec(GUID_FILE_IO, 67, TARGET, TID, 1_000, &hex(FILE_READ)));
    assert_eq!(d.finish()[0].stack, vec![0x7ff6_1000]);
}

#[test]
fn context_switch_stack_is_keyed_by_the_incoming_thread() {
    let mut d = Decoder::new(TARGET, [444]);
    d.feed(&rec(GUID_THREAD, 36, 0, 0, 3_000, &hex(CSWITCH)));
    let s = stack_record(3_000, 444, &[0x7ff6_9000]);
    d.feed(&rec(GUID_STACK_WALK, 32, 0, 0, 3_010, &s));
    assert_eq!(d.finish()[0].stack, vec![0x7ff6_9000]);
}

#[test]
fn stack_for_an_unrelated_thread_is_ignored() {
    let mut d = Decoder::new(TARGET, []);
    let s = stack_record(1_000, 31337, &[0x7ff6_1000]);
    d.feed(&rec(GUID_STACK_WALK, 32, 0, 0, 1_050, &s));
    assert!(d.finish().is_empty());
}

#[test]
fn image_load_of_the_target_decodes() {
    let mut d = Decoder::new(TARGET, []);
    let mut i = Vec::new();
    i.extend(0x7ffd_785d_0000u64.to_le_bytes());
    i.extend(0x5b_0000u64.to_le_bytes());
    i.extend(TARGET.to_le_bytes());
    i.resize(56, 0);
    i.extend(wide(
        "\\Device\\HarddiskVolume3\\Windows\\System32\\kernel32.dll",
    ));
    d.feed(&rec(GUID_IMAGE, 10, TARGET, 1, 10, &i));
    let ev = d.finish();
    let im = names(&ev, RawKind::ImageLoad)[0].image.clone().unwrap();
    assert_eq!((im.base, im.size), (0x7ffd_785d_0000, 0x5b_0000));
    assert!(im.path.ends_with("kernel32.dll"));
}

#[test]
fn image_load_of_another_process_is_dropped() {
    let mut d = Decoder::new(TARGET, []);
    let mut i = Vec::new();
    i.extend(0x1000u64.to_le_bytes());
    i.extend(0x1000u64.to_le_bytes());
    i.extend(555u32.to_le_bytes());
    i.resize(56, 0);
    i.extend(wide("x.dll"));
    d.feed(&rec(GUID_IMAGE, 10, 555, 1, 10, &i));
    assert!(d.finish().is_empty());
}

#[test]
fn process_end_of_the_target_decodes() {
    let mut d = Decoder::new(TARGET, []);
    let mut p = vec![0u8; 8];
    p.extend(TARGET.to_le_bytes());
    d.feed(&rec(GUID_PROCESS, 2, 4, 4, 77, &p));
    let ev = d.finish();
    assert_eq!((names(&ev, RawKind::ProcessEnd).len(), ev[0].qpc), (1, 77));
}

#[test]
fn truncated_payloads_never_panic() {
    let mut d = Decoder::new(TARGET, [444]);
    for (g, op) in [
        (GUID_FILE_IO, 67u8),
        (GUID_FILE_IO, 68),
        (GUID_FILE_IO, 76),
        (GUID_FILE_IO, 36),
        (GUID_FILE_IO, 64),
        (GUID_DISK_IO, 10),
        (GUID_THREAD, 36),
        (GUID_THREAD, 50),
        (GUID_THREAD, 1),
        (GUID_STACK_WALK, 32),
        (GUID_IMAGE, 10),
        (GUID_PROCESS, 2),
    ] {
        for len in [0usize, 3, 9, 20] {
            let data = vec![0xffu8; len];
            d.feed(&rec(g, op, TARGET, 1, 1, &data));
        }
    }
    let _ = d.finish();
}

#[test]
fn nt_paths_map_to_drive_letters() {
    let map = vec![
        ("\\Device\\HarddiskVolume3".to_string(), "C:".to_string()),
        ("\\Device\\HarddiskVolume30".to_string(), "D:".to_string()),
    ];
    assert_eq!(
        nt_to_dos("\\Device\\HarddiskVolume3\\data\\x.db", &map),
        "C:\\data\\x.db"
    );
    // a longer device name must not match a shorter prefix
    assert_eq!(nt_to_dos("\\Device\\HarddiskVolume30\\a", &map), "D:\\a");
    assert_eq!(nt_to_dos("\\Device\\HarddiskVolume3", &map), "C:");
    assert_eq!(nt_to_dos("\\Device\\Other\\a", &map), "\\Device\\Other\\a");
    assert_eq!(nt_to_dos("C:\\already", &map), "C:\\already");
}

#[test]
fn thread_end_removes_the_tid_so_a_reused_tid_is_not_charged_to_the_target() {
    let mut d = Decoder::new(TARGET, [444]);
    // Thread end (2): ProcessId, TThreadId
    let mut t = Vec::new();
    t.extend(TARGET.to_le_bytes());
    t.extend(444u32.to_le_bytes());
    d.feed(&rec(GUID_THREAD, 2, TARGET, 1, 100, &t));
    // the tid is reused by another process and switches in
    d.feed(&rec(GUID_THREAD, 36, 0, 0, 3_000, &hex(CSWITCH)));
    assert!(d.finish().is_empty());
}

#[test]
fn thread_end_of_another_process_keeps_the_tid() {
    let mut d = Decoder::new(TARGET, [444]);
    let mut t = Vec::new();
    t.extend(777u32.to_le_bytes());
    t.extend(444u32.to_le_bytes());
    d.feed(&rec(GUID_THREAD, 2, 777, 1, 100, &t));
    d.feed(&rec(GUID_THREAD, 36, 0, 0, 3_000, &hex(CSWITCH)));
    assert_eq!(d.finish().len(), 1);
}

#[test]
fn event_limit_stops_recording_and_counts_the_overflow() {
    let mut d = Decoder::with_limit(TARGET, [444], 3);
    for i in 0..10 {
        d.feed(&rec(GUID_THREAD, 50, TARGET, 1, i, &hex(READY)));
    }
    assert_eq!(d.dropped(), 7);
    assert_eq!(d.finish().len(), 3);
}
