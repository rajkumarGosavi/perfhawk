//! Throwaway ETW spike: run from an ELEVATED shell, do some disk I/O while it
//! runs, and paste the output back.
//!
//!     cargo run -p perfhawk-capture --example etw_spike -- [seconds]
//!
//! It answers: does the private kernel session start, do we get file/disk I/O,
//! context-switch and ready-thread events, do stack-walk events arrive and match
//! those events (same thread + timestamp), and what do the payloads look like.

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows only");
}

#[cfg(windows)]
fn main() {
    use perfhawk_capture::etw::{is_elevated, RawTrace};
    use std::collections::{BTreeMap, HashSet};
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
    let name_of = move |g: &GUID| -> String {
        if *g == file_io {
            "FileIo".into()
        } else if *g == disk_io {
            "DiskIo".into()
        } else if *g == thread {
            "Thread".into()
        } else if *g == stack_walk {
            "StackWalk".into()
        } else {
            format!("{g:?}")
        }
    };

    // PROCESS | THREAD | IMAGE_LOAD | CSWITCH | DISK_IO | DISK_FILE_IO | DISPATCHER | FILE_IO | FILE_IO_INIT
    let flags: u32 = 0x1 | 0x2 | 0x4 | 0x10 | 0x100 | 0x200 | 0x800 | 0x0200_0000 | 0x0400_0000;

    #[derive(Default)]
    struct Stats {
        by_kind: BTreeMap<(String, u8), (u64, u8, usize, String)>,
        stackable: HashSet<(u32, i64)>,
        stack_events: u64,
        stack_matched: u64,
        stack_sample: Option<String>,
    }
    let stats = Arc::new(Mutex::new(Stats::default()));
    let s2 = stats.clone();
    let stackable = [
        (file_io, 67u8),
        (file_io, 68),
        (disk_io, 10),
        (disk_io, 11),
        (thread, 36),
        (thread, 50),
    ];

    let trace = match RawTrace::start(
        &format!("perfhawk-spike-{}", std::process::id()),
        flags,
        &stackable,
        move |r| {
            let mut s = s2.lock().unwrap();
            let k = (name_of(&r.provider), r.opcode);
            let hex: String = r.data.iter().take(48).map(|b| format!("{b:02x}")).collect();
            let e = s
                .by_kind
                .entry(k.clone())
                .or_insert((0, r.version, r.data.len(), hex));
            e.0 += 1;
            if r.provider == stack_walk {
                s.stack_events += 1;
                if r.data.len() >= 16 {
                    let ts = i64::from_le_bytes(r.data[0..8].try_into().unwrap());
                    let tid = u32::from_le_bytes(r.data[12..16].try_into().unwrap());
                    if s.stackable.contains(&(tid, ts)) {
                        s.stack_matched += 1;
                    }
                    if s.stack_sample.is_none() {
                        let n = (r.data.len() - 16) / 8;
                        s.stack_sample = Some(format!(
                            "{n} frames, stack-walk record is {} bytes",
                            r.data.len()
                        ));
                    }
                }
            } else if (r.provider == file_io && (r.opcode == 67 || r.opcode == 68))
                || (r.provider == disk_io && (r.opcode == 10 || r.opcode == 11))
                || (r.provider == thread && (r.opcode == 36 || r.opcode == 50))
            {
                s.stackable.insert((r.tid, r.qpc));
            }
        },
    ) {
        Ok(t) => t,
        Err(e) => {
            println!("start failed: {e}");
            return;
        }
    };
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
    let s = stats.lock().unwrap();
    println!(
        "lost: events={} buffers={} qpc_freq={}",
        c.events_lost, c.buffers_lost, c.qpc_freq
    );
    println!(
        "stack-walk events: {} (matched to a stackable event by thread+timestamp: {})",
        s.stack_events, s.stack_matched
    );
    if let Some(x) = &s.stack_sample {
        println!("stack sample: {x}");
    }
    println!(
        "{:<12} {:>4} {:>8} {:>3} {:>5}  first 48 payload bytes",
        "provider", "op", "count", "ver", "len"
    );
    for ((p, op), (n, v, len, hex)) in &s.by_kind {
        println!("{p:<12} {op:>4} {n:>8} {v:>3} {len:>5}  {hex}");
    }
}
