//! Small Windows helpers for the ETW provider.

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0};
use windows::Win32::Storage::FileSystem::QueryDosDeviceW;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
};
use windows::Win32::System::Performance::QueryPerformanceCounter;
use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};

/// Thread ids of `pid` right now (empty if the process is gone).
pub fn thread_ids(pid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) else {
            return out;
        };
        let mut e = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        if Thread32First(snap, &mut e).is_ok() {
            loop {
                if e.th32OwnerProcessID == pid {
                    out.push(e.th32ThreadID);
                }
                if Thread32Next(snap, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    out
}

/// False once the process has exited. When it cannot be opened for another
/// reason (access), assume it is still running.
pub fn process_alive(pid: u32) -> bool {
    unsafe {
        match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            Ok(h) => {
                let exited = WaitForSingleObject(h, 0) == WAIT_OBJECT_0;
                let _ = CloseHandle(h);
                !exited
            }
            Err(e) => e.code() != ERROR_INVALID_PARAMETER.to_hresult(),
        }
    }
}

pub fn qpc_now() -> i64 {
    let mut v = 0i64;
    unsafe {
        let _ = QueryPerformanceCounter(&mut v);
    }
    v
}

/// (`\Device\HarddiskVolume3`, `C:`) for every mapped drive letter.
pub fn dos_devices() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for letter in b'A'..=b'Z' {
        let drive = format!("{}:", letter as char);
        let wide: Vec<u16> = drive.encode_utf16().chain(std::iter::once(0)).collect();
        let mut buf = [0u16; 1024];
        let n = unsafe { QueryDosDeviceW(PCWSTR(wide.as_ptr()), Some(&mut buf)) } as usize;
        if n > 0 {
            let end = buf.iter().position(|&c| c == 0).unwrap_or(n);
            out.push((String::from_utf16_lossy(&buf[..end]), drive));
        }
    }
    out
}
