//! Parse .nv.info sections from CUBIN ELF files.
//!
//! NVIDIA embeds kernel parameter metadata in `.nv.info` and `.nv.info.<kernel>`
//! ELF sections (type `SHT_CUDA_INFO = 0x70000001`).  This module extracts:
//!
//! - `EIATTR_PARAM_CBANK`  — constant-bank offset & size for kernel parameters
//! - `EIATTR_KPARAM_INFO`  — per-parameter index, ordinal, size, flags
//! - `EIATTR_CBANK_PARAM_SIZE` — global total param size in the cbank
//!
//! SM89 / SM120 ABI difference:
//!   - SM89  kernel params start at `c[0x0][0x160]`
//!   - SM120 kernel params start at `c[0x0][0x380]`
//!   The actual offset is read from `EIATTR_PARAM_CBANK` at runtime.

use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// One KPARAM_INFO entry for a kernel parameter.
///
/// Layout decoded from ptxas-13.0.88 binary analysis:
///   bytes [0:4]   u32 index      — symbol index (0 for common case)
///   bytes [4:6]   u16 ordinal    — parameter position
///   bytes [6:8]   u16 offset     — byte offset within the param constant bank
///   bytes [8:12]  u32 packed:
///                   logAlign  : 8   (bits 0-7)
///                   space     : 4   (bits 8-11)
///                   cbankid   : 5   (bits 12-16)
///                   isSmemParam : 1 (bit 17)
///                   size      : 14  (bits 18-31)
///
/// Ref: platform/crucible-notes/ptxas/wiki/src/output/sections.md § EIATTR_KPARAM_INFO Bitfield
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvKparam {
    /// Symbol index (usually 0 for kernel parameters).
    pub index: u32,
    /// Ordinal position in the parameter list (0 = first, 1 = second, …).
    pub ordinal: u16,
    /// Byte offset of this parameter within the constant bank.
    pub offset: u16,
    /// log2(alignment) for this parameter.
    pub log_align: u8,
    /// Address space (0 = cbank, …).
    pub space: u8,
    /// Constant bank ID (typically 0).
    pub cbankid: u8,
    /// True if this parameter lives in shared memory.
    pub is_smem_param: bool,
    /// Size of this parameter in bytes.
    pub size: u16,
}

/// Kernel-level metadata extracted from .nv.info.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvKernelInfo {
    pub name: String,
    /// Offset within constant bank 0 where kernel parameters begin.
    pub param_cbank_offset: u16,
    /// Total size in bytes of the parameter region.
    pub param_cbank_size: u16,
    /// Per-parameter descriptors.
    pub kparams: Vec<NvKparam>,
}

// ---------------------------------------------------------------------------
// ELF helpers (minimal 64-bit parser)
// ---------------------------------------------------------------------------

const EI_NIDENT: usize = 16;
const ELFCLASS64: u8 = 2;

// Section header types we care about
const SHT_CUDA_INFO: u32 = 0x7000_0000;

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}

fn read_u64_le(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        data[offset], data[offset + 1], data[offset + 2], data[offset + 3],
        data[offset + 4], data[offset + 5], data[offset + 6], data[offset + 7],
    ])
}

struct SectionHeader {
    name_offset: u32,
    sh_type: u32,
    sh_offset: u64,
    sh_size: u64,
}

/// Parse the 64-bit ELF header and return (shoff, shentsize, shnum, shstrndx).
fn elf_header_64(data: &[u8]) -> Result<(u64, u16, u16, u16), &'static str> {
    if data.len() < 64 || &data[..4] != b"\x7fELF" {
        return Err("Not an ELF file");
    }
    if data[4] != ELFCLASS64 {
        return Err("Not a 64-bit ELF file");
    }
    let shoff = read_u64_le(data, 0x28);
    let shentsize = read_u16_le(data, 0x3A);
    let shnum = read_u16_le(data, 0x3C);
    let shstrndx = read_u16_le(data, 0x3E);
    Ok((shoff, shentsize, shnum, shstrndx))
}

/// Read a null-terminated string from `shstrtab` at `offset`.
fn section_name(shstrtab: &[u8], offset: u32) -> &str {
    let start = offset as usize;
    let end = shstrtab[start..]
        .iter()
        .position(|&b| b == 0)
        .map(|p| start + p)
        .unwrap_or(shstrtab.len());
    std::str::from_utf8(&shstrtab[start..end]).unwrap_or("")
}

// ---------------------------------------------------------------------------
// .nv.info attribute parsing
// ---------------------------------------------------------------------------

/// Attribute IDs we decode.
const EIATTR_PARAM_CBANK: u8 = 0x0a;
const EIATTR_CBANK_PARAM_SIZE: u8 = 0x19;
const EIATTR_KPARAM_INFO: u8 = 0x17;

/// Attribute format: 0x04 = per-function, 0x03 = global.
const ATTR_FMT_PER_FUNC: u8 = 0x04;
const ATTR_FMT_GLOBAL: u8 = 0x03;

/// Walk a single .nv.info section payload and collect kernel metadata.
fn parse_nvinfo_payload(
    payload: &[u8],
    kernel_name: Option<&str>,
    kernels: &mut BTreeMap<String, NvKernelInfo>,
    global: &mut GlobalNvInfo,
) {
    let mut pos = 0usize;
    while pos + 4 <= payload.len() {
        let fmt_byte = payload[pos];
        let attr_id = payload[pos + 1];

        // Per-function (0x04): header = {fmt:1, attr:1, size:2}, then payload.
        // Global       (0x03): header = {fmt:1, attr:1, val:2} — the "size"
        //                       field IS the value, no separate payload.
        match fmt_byte {
            ATTR_FMT_PER_FUNC => {
                let attr_size = read_u16_le(payload, pos + 2) as usize;
                pos += 4;
                if attr_size == 0 || pos + attr_size > payload.len() {
                    break;
                }
                let attr_data = &payload[pos..pos + attr_size];
                pos += attr_size;

                match attr_id {
                    EIATTR_PARAM_CBANK => {
                        if let (Some(name), true) = (kernel_name, attr_data.len() >= 8) {
                            let offset = read_u16_le(attr_data, 4);
                            let size = read_u16_le(attr_data, 6);
                            let info = kernels.entry(name.to_string()).or_insert_with(|| {
                                NvKernelInfo {
                                    name: name.to_string(),
                                    param_cbank_offset: offset,
                                    param_cbank_size: size,
                                    kparams: Vec::new(),
                                }
                            });
                            info.param_cbank_offset = offset;
                            info.param_cbank_size = size;
                        }
                    }
                    EIATTR_KPARAM_INFO => {
                        if let (Some(name), true) = (kernel_name, attr_data.len() >= 12) {
                            // Real layout (from ptxas binary analysis):
                            //   [0:4] u32 index, [4:6] u16 ordinal, [6:8] u16 offset,
                            //   [8:12] u32 packed { logAlign:8, space:4, cbankid:5, isSmemParam:1, size:14 }
                            let index = read_u32_le(attr_data, 0);
                            let ordinal = read_u16_le(attr_data, 4);
                            let offset = read_u16_le(attr_data, 6);
                            let packed = read_u32_le(attr_data, 8);
                            let log_align = (packed & 0xFF) as u8;
                            let space = ((packed >> 8) & 0xF) as u8;
                            let cbankid = ((packed >> 12) & 0x1F) as u8;
                            let is_smem_param = ((packed >> 17) & 0x1) != 0;
                            let size = ((packed >> 18) & 0x3FFF) as u16;
                            let info = kernels.entry(name.to_string()).or_insert_with(|| {
                                NvKernelInfo {
                                    name: name.to_string(),
                                    param_cbank_offset: 0x160,
                                    param_cbank_size: 0,
                                    kparams: Vec::new(),
                                }
                            });
                            info.kparams.push(NvKparam {
                                index, ordinal, offset,
                                log_align, space, cbankid, is_smem_param, size,
                            });
                        }
                    }
                    _ => {} // ignore unknown per-func attrs
                }
            }
            ATTR_FMT_GLOBAL => {
                // Global attributes: the u16 at pos+2 is the value itself.
                let value = read_u16_le(payload, pos + 2);
                pos += 4; // consume the 4-byte header
                match attr_id {
                    EIATTR_PARAM_CBANK => {
                        // First u16 = offset, next u16 = size (if present)
                        global.param_cbank_offset = Some(value);
                    }
                    EIATTR_CBANK_PARAM_SIZE => {
                        global.cbank_param_size = Some(value);
                    }
                    _ => {} // ignore
                }
            }
            _ => {
                // Unknown format — try to skip using the u16 "size" heuristic
                let attr_size = read_u16_le(payload, pos + 2) as usize;
                pos += 4;
                if pos + attr_size > payload.len() {
                    break;
                }
                pos += attr_size;
            }
        }
    }
}

struct GlobalNvInfo {
    param_cbank_offset: Option<u16>,
    param_cbank_size: Option<u16>,
    cbank_param_size: Option<u16>,
}

impl Default for GlobalNvInfo {
    fn default() -> Self {
        Self { param_cbank_offset: None, param_cbank_size: None, cbank_param_size: None }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse all .nv.info* sections from a cubin ELF and return per-kernel metadata.
///
/// Returns a map from kernel name → `NvKernelInfo`.
pub fn parse_cubin_nvinfo(cubin_data: &[u8]) -> Result<BTreeMap<String, NvKernelInfo>, String> {
    let (shoff, shentsize, shnum, shstrndx) = elf_header_64(cubin_data)
        .map_err(|e| format!("ELF header: {}", e))?;

    // Read section name string table
    let shstr_hdr_off = shoff as usize + shstrndx as usize * shentsize as usize;
    if shstr_hdr_off + shentsize as usize > cubin_data.len() {
        return Err("Section header string table out of bounds".into());
    }
    let shstr_offset = read_u64_le(cubin_data, shstr_hdr_off + 0x18);
    let shstr_size = read_u64_le(cubin_data, shstr_hdr_off + 0x20);
    if shstr_offset as usize + shstr_size as usize > cubin_data.len() {
        return Err("String table out of bounds".into());
    }
    let shstrtab = &cubin_data[shstr_offset as usize..(shstr_offset + shstr_size) as usize];

    let mut kernels: BTreeMap<String, NvKernelInfo> = BTreeMap::new();
    let mut global = GlobalNvInfo::default();

    for i in 0..shnum {
        let hdr_off = shoff as usize + i as usize * shentsize as usize;
        if hdr_off + shentsize as usize > cubin_data.len() {
            continue;
        }
        let name_idx = read_u32_le(cubin_data, hdr_off);
        let sh_type = read_u32_le(cubin_data, hdr_off + 4);
        let sh_offset = read_u64_le(cubin_data, hdr_off + 0x18);
        let sh_size = read_u64_le(cubin_data, hdr_off + 0x20);

        let name = section_name(shstrtab, name_idx);

        // Only process .nv.info* sections
        if !name.starts_with(".nv.info") || sh_type != SHT_CUDA_INFO {
            continue;
        }
        if sh_size == 0 || sh_offset as usize + sh_size as usize > cubin_data.len() {
            continue;
        }

        let payload = &cubin_data[sh_offset as usize..(sh_offset + sh_size) as usize];
        let kernel_name = name.strip_prefix(".nv.info.").filter(|n| !n.is_empty());
        parse_nvinfo_payload(payload, kernel_name, &mut kernels, &mut global);
    }

    // Apply global defaults to kernels that didn't have explicit PARAM_CBANK
    let default_offset = global.param_cbank_offset.unwrap_or(0x160);
    let default_size = global
        .param_cbank_size
        .or(global.cbank_param_size)
        .unwrap_or(0);

    for info in kernels.values_mut() {
        if info.param_cbank_offset == 0 {
            info.param_cbank_offset = default_offset;
        }
        if info.param_cbank_size == 0 {
            info.param_cbank_size = default_size;
        }
        // Sort kparams by ordinal for deterministic output
        info.kparams.sort_by_key(|p| p.ordinal);
    }

    Ok(kernels)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elf_header_rejects_non_elf() {
        assert!(elf_header_64(b"not an elf file at all").is_err());
    }

    #[test]
    fn test_empty_cubin_returns_empty_map() {
        let mut elf = vec![0u8; 64];
        elf[0] = 0x7f; elf[1] = b'E'; elf[2] = b'L'; elf[3] = b'F';
        elf[4] = ELFCLASS64;
        let kernels = parse_cubin_nvinfo(&elf).unwrap();
        assert!(kernels.is_empty());
    }

    #[test]
    fn test_int_add_cubin_nvinfo() {
        // Use workspace-relative path (hetGPU is in lib/qemu/subprojects/hetGPU/)
        let cubin = std::fs::read(
            "/workspace/platform/data/sm89-sass-dumps/quick/cubin/int_add.cubin"
        ).expect("int_add.cubin should exist");
        let kernels = parse_cubin_nvinfo(&cubin).expect("parse should succeed");
        eprintln!("kernels: {:#?}", kernels);
        let info = kernels.get("int_add").expect("int_add kernel should be found");
        eprintln!("int_add info: {:?}", info);
        assert!(info.param_cbank_offset == 0x160, "cbank offset should be 0x160, got 0x{:x}", info.param_cbank_offset);
        assert!(info.kparams.len() >= 2, "should have at least 2 kparams");
    }
}
