//! PE data-directory walk to the READYTORUN header and its section list.

use object::pe::{IMAGE_DIRECTORY_ENTRY_COM_DESCRIPTOR, IMAGE_FILE_MACHINE_AMD64};
use object::read::pe::{ImageNtHeaders, PeFile64};
use std::ops::Range;

/// `READYTORUN_HEADER.Signature`: "RTR\0".
const RTR_SIGNATURE: u32 = 0x0052_5452;
const SECTION_RUNTIME_FUNCTIONS: u32 = 102;
const SECTION_METHOD_DEF_ENTRY_POINTS: u32 = 103;
/// Offset of `ManagedNativeHeader` inside `IMAGE_COR20_HEADER`.
const COR_MANAGED_NATIVE_HEADER: usize = 64;
/// Offset of `MetaData` inside `IMAGE_COR20_HEADER`.
const COR_METADATA: usize = 8;

/// Where the tables of a ReadyToRun image live, as byte ranges into the file.
#[derive(Debug, Clone)]
pub struct R2rImage<'a> {
    pub data: &'a [u8],
    pub major: u16,
    pub minor: u16,
    /// 12-byte `RUNTIME_FUNCTION` entries.
    pub runtime_functions: Range<usize>,
    /// Native-format sparse array indexed by MethodDef row.
    pub method_def_entry_points: Range<usize>,
    /// The CLI metadata directory from the COR header.
    pub metadata: Range<usize>,
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

/// Parse a ReadyToRun image (x64 only). `None` for anything else, including
/// truncated or malformed input.
pub fn open(data: &[u8]) -> Option<R2rImage<'_>> {
    let pe = PeFile64::parse(data).ok()?;
    if pe
        .nt_headers()
        .file_header()
        .machine
        .get(object::LittleEndian)
        != IMAGE_FILE_MACHINE_AMD64
    {
        return None;
    }
    let sections = pe.section_table();
    let range = |rva: u32, size: u32| -> Option<Range<usize>> {
        let (off, avail) = sections.pe_file_range_at(rva)?;
        let size = size.min(avail);
        let start = off as usize;
        let end = start.checked_add(size as usize)?;
        (end <= data.len()).then_some(start..end)
    };
    let dirs = pe.data_directories();
    let com = dirs.get(IMAGE_DIRECTORY_ENTRY_COM_DESCRIPTOR)?;
    let (com_rva, com_size) = (
        com.virtual_address.get(object::LittleEndian),
        com.size.get(object::LittleEndian),
    );
    if com_size < 72 {
        return None;
    }
    let cor = &data[range(com_rva, com_size)?];
    let native_rva = u32_at(cor, COR_MANAGED_NATIVE_HEADER)?;
    let native_size = u32_at(cor, COR_MANAGED_NATIVE_HEADER + 4)?;
    let meta_rva = u32_at(cor, COR_METADATA)?;
    let meta_size = u32_at(cor, COR_METADATA + 4)?;
    if native_rva == 0 {
        return None;
    }
    let hdr = &data[range(native_rva, native_size)?];
    if u32_at(hdr, 0)? != RTR_SIGNATURE {
        return None;
    }
    let (major, minor) = (u16_at(hdr, 4)?, u16_at(hdr, 6)?);
    if major < 2 {
        return None;
    }
    // READYTORUN_CORE_HEADER: u32 flags, u32 section count, then sections.
    let count = u32_at(hdr, 12)? as usize;
    let (mut rf, mut md) = (None, None);
    for i in 0..count.min(256) {
        let o = 16 + i * 12;
        let (ty, rva, size) = (u32_at(hdr, o)?, u32_at(hdr, o + 4)?, u32_at(hdr, o + 8)?);
        match ty {
            SECTION_RUNTIME_FUNCTIONS => rf = range(rva, size),
            SECTION_METHOD_DEF_ENTRY_POINTS => md = range(rva, size),
            _ => {}
        }
    }
    Some(R2rImage {
        data,
        major,
        minor,
        runtime_functions: rf?,
        method_def_entry_points: md?,
        metadata: range(meta_rva, meta_size)?,
    })
}
