//! Turns raw Windows kernel-trace records into [`RawEvent`]s for one target
//! process. Pure byte decoding (no Windows APIs), so it is tested everywhere
//! with payloads captured from a real trace.
//!
//! Layouts (offsets in bytes, little endian, 64-bit) were read from a live
//! trace of Windows 11; see docs/perf/etw-spike.md.

use crate::sysevents::{ImageInfo, RawEvent, RawKind};
use std::collections::{HashMap, HashSet};

pub const GUID_FILE_IO: u128 = 0x90cbdc39_4a3e_11d1_84f4_0000f80464e3;
pub const GUID_DISK_IO: u128 = 0x3d6fa8d4_fe05_11d0_9dda_00c04fd7ba7c;
pub const GUID_THREAD: u128 = 0x3d6fa8d1_fe05_11d0_9dda_00c04fd7ba7c;
pub const GUID_PROCESS: u128 = 0x3d6fa8d0_fe05_11d0_9dda_00c04fd7ba7c;
pub const GUID_IMAGE: u128 = 0x2cb15d1d_5fc1_11d2_abe1_00a0c911f518;
pub const GUID_STACK_WALK: u128 = 0xdef2fe46_7bd6_4b80_bd94_f57fe20d0ce3;

/// Kernel frames start here; they are dropped from stacks.
const KERNEL_BASE: u64 = 0xffff_8000_0000_0000;
/// Bound on stacks that arrived before their event.
const MAX_EARLY_STACKS: usize = 200_000;

/// One record from the trace, in the shape the decoder needs.
pub struct Record<'a> {
    pub provider: u128,
    pub opcode: u8,
    pub version: u8,
    pub pid: u32,
    pub tid: u32,
    /// Raw QPC ticks.
    pub qpc: i64,
    pub data: &'a [u8],
}

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

fn u64_at(d: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(d.get(o..o + 8)?.try_into().ok()?))
}

/// A NUL-terminated UTF-16 string starting at `o`.
fn wstr_at(d: &[u8], o: usize) -> Option<String> {
    let rest = d.get(o..)?;
    let units: Vec<u16> = rest
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    Some(String::from_utf16_lossy(&units))
}

pub struct Decoder {
    target_pid: u32,
    tids: HashSet<u32>,
    events: Vec<RawEvent>,
    /// Events still waiting for their stack-walk record: (raw time, thread).
    pending: HashMap<(i64, u32), usize>,
    /// Stack-walk records that arrived before their event.
    early: HashMap<(i64, u32), Vec<u64>>,
    /// In-flight file operations by IRP, to time them at completion.
    irps: HashMap<u64, usize>,
    /// (event index, file key, file object) for name resolution at the end.
    file_refs: Vec<(usize, u64, u64)>,
    names_by_key: HashMap<u64, String>,
    names_by_object: HashMap<u64, String>,
}

impl Decoder {
    /// `seed_tids`: the target's threads at capture start.
    pub fn new(target_pid: u32, seed_tids: impl IntoIterator<Item = u32>) -> Self {
        Decoder {
            target_pid,
            tids: seed_tids.into_iter().collect(),
            events: Vec::new(),
            pending: HashMap::new(),
            early: HashMap::new(),
            irps: HashMap::new(),
            file_refs: Vec::new(),
            names_by_key: HashMap::new(),
            names_by_object: HashMap::new(),
        }
    }

    pub fn feed(&mut self, r: &Record) {
        let _ = match r.provider {
            GUID_FILE_IO => self.file_io(r),
            GUID_DISK_IO => self.disk_io(r),
            GUID_THREAD => self.thread(r),
            GUID_STACK_WALK => self.stack_walk(r),
            GUID_IMAGE => self.image(r),
            GUID_PROCESS => self.process(r),
            _ => None,
        };
    }

    fn push(&mut self, e: RawEvent) -> usize {
        self.events.push(e);
        self.events.len() - 1
    }

    /// Register an event that gets its stack from a later (or earlier) stack-walk record.
    fn expect_stack(&mut self, idx: usize, qpc: i64, tid: u32) {
        match self.early.remove(&(qpc, tid)) {
            Some(frames) => self.events[idx].stack = frames,
            None => {
                self.pending.insert((qpc, tid), idx);
            }
        }
    }

    fn file_io(&mut self, r: &Record) -> Option<()> {
        let d = r.data;
        match r.opcode {
            67 | 68 => {
                let ttid = u32_at(d, 32)?;
                if r.pid != self.target_pid && !self.tids.contains(&ttid) {
                    return None;
                }
                self.tids.insert(ttid);
                let kind = if r.opcode == 67 {
                    RawKind::FileRead
                } else {
                    RawKind::FileWrite
                };
                let mut e = RawEvent::new(kind, r.qpc, self.target_pid, ttid);
                e.bytes = u32_at(d, 36)? as u64;
                let irp = u64_at(d, 8)?;
                let (obj, key) = (u64_at(d, 16)?, u64_at(d, 24)?);
                let idx = self.push(e);
                self.irps.insert(irp, idx);
                self.file_refs.push((idx, key, obj));
                self.expect_stack(idx, r.qpc, r.tid);
            }
            76 => {
                let idx = self.irps.remove(&u64_at(d, 0)?)?;
                self.events[idx].duration_qpc = (r.qpc - self.events[idx].qpc).max(0);
            }
            0 | 32 | 35 | 36 => {
                self.names_by_key.insert(u64_at(d, 0)?, wstr_at(d, 8)?);
            }
            64 => {
                self.names_by_object.insert(u64_at(d, 8)?, wstr_at(d, 32)?);
            }
            _ => {}
        }
        Some(())
    }

    fn disk_io(&mut self, r: &Record) -> Option<()> {
        if r.opcode != 10 && r.opcode != 11 {
            return None;
        }
        let d = r.data;
        let issuer = u32_at(d, 48)?;
        if !self.tids.contains(&issuer) {
            return None;
        }
        let kind = if r.opcode == 10 {
            RawKind::DiskRead
        } else {
            RawKind::DiskWrite
        };
        let mut e = RawEvent::new(kind, r.qpc, self.target_pid, issuer);
        e.bytes = u32_at(d, 8)? as u64;
        e.duration_qpc = u64_at(d, 40)? as i64;
        let obj = u64_at(d, 24)?;
        let idx = self.push(e);
        self.file_refs.push((idx, 0, obj));
        Some(())
    }

    fn thread(&mut self, r: &Record) -> Option<()> {
        let d = r.data;
        match r.opcode {
            // start, rundown start: ProcessId, TThreadId
            1 | 3 => {
                if u32_at(d, 0)? == self.target_pid {
                    self.tids.insert(u32_at(d, 4)?);
                }
            }
            36 => {
                let (new, old) = (u32_at(d, 0)?, u32_at(d, 4)?);
                let new_ours = self.tids.contains(&new);
                if !new_ours && !self.tids.contains(&old) {
                    return None;
                }
                let pid = if new_ours { self.target_pid } else { 0 };
                let mut e = RawEvent::new(RawKind::CSwitch, r.qpc, pid, new);
                e.new_tid = new;
                e.old_tid = old;
                e.wait_reason = *d.get(12)?;
                let idx = self.push(e);
                if new_ours {
                    self.expect_stack(idx, r.qpc, new);
                }
            }
            50 => {
                let tid = u32_at(d, 0)?;
                if !self.tids.contains(&tid) {
                    return None;
                }
                let mut e = RawEvent::new(RawKind::ReadyThread, r.qpc, self.target_pid, tid);
                e.new_tid = tid;
                self.push(e);
            }
            _ => {}
        }
        Some(())
    }

    fn stack_walk(&mut self, r: &Record) -> Option<()> {
        if r.opcode != 32 {
            return None;
        }
        let d = r.data;
        let (ts, tid) = (u64_at(d, 0)? as i64, u32_at(d, 12)?);
        let key = (ts, tid);
        if !self.pending.contains_key(&key)
            && (!self.tids.contains(&tid) || self.early.len() >= MAX_EARLY_STACKS)
        {
            return None;
        }
        let frames: Vec<u64> = d
            .get(16..)?
            .chunks_exact(8)
            .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
            .filter(|&a| a < KERNEL_BASE)
            .collect();
        match self.pending.remove(&key) {
            Some(idx) => self.events[idx].stack = frames,
            None => {
                self.early.insert(key, frames);
            }
        }
        Some(())
    }

    fn image(&mut self, r: &Record) -> Option<()> {
        // load, unload, rundown start, rundown end
        if !matches!(r.opcode, 2 | 3 | 4 | 10) {
            return None;
        }
        let d = r.data;
        if u32_at(d, 16)? != self.target_pid {
            return None;
        }
        let mut e = RawEvent::new(RawKind::ImageLoad, r.qpc, self.target_pid, r.tid);
        e.image = Some(ImageInfo {
            base: u64_at(d, 0)?,
            size: u64_at(d, 8)?,
            path: wstr_at(d, 56)?,
        });
        self.push(e);
        Some(())
    }

    fn process(&mut self, r: &Record) -> Option<()> {
        if r.opcode == 2 && u32_at(r.data, 8)? == self.target_pid {
            self.push(RawEvent::new(
                RawKind::ProcessEnd,
                r.qpc,
                self.target_pid,
                r.tid,
            ));
        }
        Some(())
    }

    /// Resolve file names (the trace names files late, in rundown) and hand
    /// back the events in arrival order.
    pub fn finish(mut self) -> Vec<RawEvent> {
        for (idx, key, obj) in std::mem::take(&mut self.file_refs) {
            self.events[idx].file = self
                .names_by_key
                .get(&key)
                .or_else(|| self.names_by_object.get(&obj))
                .cloned();
        }
        self.events
    }
}

/// `\Device\HarddiskVolume3\a` to `C:\a`, given (device, drive) pairs. Paths
/// on unknown devices are returned unchanged.
pub fn nt_to_dos(path: &str, devices: &[(String, String)]) -> String {
    for (dev, drive) in devices {
        if let Some(rest) = path.strip_prefix(dev.as_str()) {
            if rest.is_empty() || rest.starts_with('\\') {
                return format!("{drive}{rest}");
            }
        }
    }
    path.to_string()
}
