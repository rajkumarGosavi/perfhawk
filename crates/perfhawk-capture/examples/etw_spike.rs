//! Throwaway ETW spike (v2): run from an ELEVATED shell and paste the output.
//!
//!     cargo run -p perfhawk-capture --example etw_spike -- [seconds]
//!
//! Round 1 showed events and stack-walk records arrive, but no stack-walk record
//! matched an event by (thread, timestamp). This round checks the time base and
//! which thread id a stack belongs to for each event type.

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows only");
}

#[cfg(windows)]
fn main() {
    use perfhawk_capture::etw::{is_elevated, RawTrace};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use windows::core::GUID;

    let secs: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    println!("elevated: {}", is_elevated());

    let file_io = GUID::from_u128(0x90cbdc39_4a3e_11d1_84f4_0000f80464e3);
    let disk_io = GUID::from_u128(0x3d6fa8d4_fe05_11d0_9dda_00c04fd7ba7c);
    let thread = GUID::from_u128(0x3d6fa8d1_fe05_11d0_9dda_00c04fd7ba7c);
    let stack_walk = GUID::from_u128(0xdef2fe46_7bd6_4b80_bd94_f57fe20d0ce3);

    // PROCESS | THREAD | IMAGE_LOAD | CSWITCH | DISK_IO | DISK_FILE_IO | DISPATCHER | FILE_IO | FILE_IO_INIT
    let flags: u32 = 0x1 | 0x2 | 0x4 | 0x10 | 0x100 | 0x200 | 0x800 | 0x0200_0000 | 0x0400_0000;

    /// A stackable event: what it is, header thread/pid, payload thread ids.
    struct Ev {
        label: &'static str,
        hdr_tid: u32,
        hdr_pid: u32,
        payload_tids: [u32; 2],
    }
    #[derive(Default)]
    struct Data {
        events: HashMap<i64, Vec<Ev>>,
        stacks: Vec<(i64, u32, u32, usize)>,
        first_event_qpc: Vec<i64>,
    }
    let data = Arc::new(Mutex::new(Data::default()));
    let d2 = data.clone();
    let stackable = [
        (file_io, 67u8),
        (file_io, 68),
        (disk_io, 10),
        (disk_io, 11),
        (thread, 36),
        (thread, 50),
    ];
    let u32at = |d: &[u8], o: usize| -> u32 {
        d.get(o..o + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .unwrap_or(0)
    };

    let trace = match RawTrace::start(
        &format!("perfhawk-spike-{}", std::process::id()),
        flags,
        &stackable,
        move |r| {
            let mut s = d2.lock().unwrap();
            let (label, payload_tids) = if r.provider == stack_walk {
                if r.data.len() >= 16 {
                    let ts = i64::from_le_bytes(r.data[0..8].try_into().unwrap());
                    s.stacks.push((
                        ts,
                        u32at(r.data, 8),
                        u32at(r.data, 12),
                        (r.data.len() - 16) / 8,
                    ));
                }
                return;
            } else if r.provider == file_io && (r.opcode == 67 || r.opcode == 68) {
                (
                    if r.opcode == 67 {
                        "FileRead"
                    } else {
                        "FileWrite"
                    },
                    [u32at(r.data, 32), 0],
                )
            } else if r.provider == disk_io && (r.opcode == 10 || r.opcode == 11) {
                (
                    if r.opcode == 10 {
                        "DiskRead"
                    } else {
                        "DiskWrite"
                    },
                    [u32at(r.data, 48), 0],
                )
            } else if r.provider == thread && r.opcode == 36 {
                ("CSwitch", [u32at(r.data, 4), u32at(r.data, 0)]) // old, new
            } else if r.provider == thread && r.opcode == 50 {
                ("Ready", [u32at(r.data, 0), 0]) // readied thread
            } else {
                return;
            };
            if s.first_event_qpc.len() < 3 {
                s.first_event_qpc.push(r.qpc);
            }
            s.events.entry(r.qpc).or_default().push(Ev {
                label,
                hdr_tid: r.tid,
                hdr_pid: r.pid,
                payload_tids,
            });
        },
    ) {
        Ok(t) => t,
        Err(e) => {
            println!("start failed: {e}");
            return;
        }
    };
    println!(
        "session mode: {}",
        if trace.classic {
            "classic NT Kernel Logger"
        } else {
            "private system logger"
        }
    );
    println!("tracing for {secs}s; doing some file I/O of our own meanwhile");
    let dir = std::env::temp_dir().join("perfhawk-spike");
    let _ = std::fs::create_dir_all(&dir);
    let end = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    let mut i = 0u32;
    while std::time::Instant::now() < end {
        let f = dir.join(format!("f{}.bin", i % 4));
        let _ = std::fs::write(&f, vec![7u8; 1 << 20]);
        if let Ok(h) = std::fs::OpenOptions::new().write(true).open(&f) {
            let _ = h.sync_all();
        }
        let _ = std::fs::read(&f);
        i += 1;
    }
    let c = trace.stop();
    let s = data.lock().unwrap();
    println!(
        "lost: events={} buffers={} qpc_freq={}",
        c.events_lost, c.buffers_lost, c.qpc_freq
    );
    println!("our pid: {}", std::process::id());
    println!(
        "stackable events: {} distinct timestamps; stack records: {}",
        s.events.len(),
        s.stacks.len()
    );
    println!("first stackable event qpc: {:?}", s.first_event_qpc);
    let sts: Vec<i64> = s.stacks.iter().take(3).map(|x| x.0).collect();
    println!("first stack-walk EventTimeStamp: {sts:?}");

    // Match by timestamp alone, then classify which thread id agrees.
    let mut by_label: HashMap<&str, [u64; 5]> = HashMap::new(); // total, ts match, hdr tid, payload0, payload1
    let mut unmatched_ts = 0u64;
    let mut sample_unmatched: Vec<i64> = vec![];
    for (ts, _pid, tid, _n) in &s.stacks {
        match s.events.get(ts) {
            None => {
                unmatched_ts += 1;
                if sample_unmatched.len() < 3 {
                    sample_unmatched.push(*ts);
                }
            }
            Some(evs) => {
                for e in evs {
                    let a = by_label.entry(e.label).or_insert([0; 5]);
                    a[1] += 1;
                    if e.hdr_tid == *tid {
                        a[2] += 1;
                    }
                    if e.payload_tids[0] == *tid {
                        a[3] += 1;
                    }
                    if e.payload_tids[1] == *tid {
                        a[4] += 1;
                    }
                }
            }
        }
    }
    for (label, n) in s
        .events
        .values()
        .flatten()
        .fold(HashMap::<&str, u64>::new(), |mut m, e| {
            *m.entry(e.label).or_default() += 1;
            m
        })
    {
        by_label.entry(label).or_insert([0; 5])[0] = n;
    }
    println!("stack records whose timestamp matches no stackable event: {unmatched_ts} (samples {sample_unmatched:?})");
    println!("event type | events | stack ts-matches | tid==header tid | tid==payload[0] | tid==payload[1]");
    for (l, a) in &by_label {
        println!(
            "{l:<10} | {:>6} | {:>6} | {:>6} | {:>6} | {:>6}",
            a[0], a[1], a[2], a[3], a[4]
        );
    }
    let pids: HashMap<u32, u64> = s
        .events
        .values()
        .flatten()
        .filter(|e| e.label == "FileWrite")
        .fold(HashMap::new(), |mut m, e| {
            *m.entry(e.hdr_pid).or_default() += 1;
            m
        });
    println!(
        "FileWrite header pids (expect our pid {}): {pids:?}",
        std::process::id()
    );
}
