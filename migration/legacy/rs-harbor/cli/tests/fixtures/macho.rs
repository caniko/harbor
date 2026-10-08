//! Minimal Mach-O dylib byte builder for audit tests.
//!
//! Goblin only needs valid header magic + a coherent load-command stream
//! to surface `macho.libs`. We build a 64-bit thin Mach-O with one
//! `LC_ID_DYLIB` and N `LC_LOAD_DYLIBs`; goblin reads the dependency strings
//! out of those load commands and that's all `audit macho` inspects.

use std::io::Write;

const MH_MAGIC_64: u32 = 0xFEED_FACF;
const CPU_TYPE_X86_64: i32 = 0x0100_0007;
const CPU_SUBTYPE_X86_64_ALL: i32 = 3;
const MH_EXECUTE: u32 = 2;
const MH_DYLIB: u32 = 6;

const LC_ID_DYLIB: u32 = 0xD;
const LC_LOAD_DYLIB: u32 = 0xC;

fn dylib_command(cmd: u32, name: &str) -> Vec<u8> {
    // dylib_command struct (24 bytes) + name (null-terminated, padded to 8):
    //   uint32_t cmd
    //   uint32_t cmdsize
    //   struct dylib {
    //     union lc_str name (offset = 24)
    //     uint32_t timestamp
    //     uint32_t current_version
    //     uint32_t compatibility_version
    //   }
    let name_bytes_len = name.len() + 1; // null terminator
    let total_unpadded = 24 + name_bytes_len;
    let cmdsize = total_unpadded.div_ceil(8) * 8;
    let pad = cmdsize - total_unpadded;

    let mut out = Vec::with_capacity(cmdsize);
    out.extend(cmd.to_le_bytes());
    #[allow(clippy::cast_possible_truncation)]
    out.extend((cmdsize as u32).to_le_bytes());
    out.extend(24u32.to_le_bytes()); // name offset within this command
    out.extend(0u32.to_le_bytes()); // timestamp
    out.extend(0x0001_0000u32.to_le_bytes()); // current_version (1.0.0)
    out.extend(0x0001_0000u32.to_le_bytes()); // compatibility_version
    out.extend(name.as_bytes());
    out.push(0);
    out.extend(std::iter::repeat_n(0u8, pad));
    out
}

/// Build a minimal Mach-O 64-bit thin dylib with `id` as the install
/// name and `loads` as the `LC_LOAD_DYLIB` dependencies.
pub fn build_thin_dylib(id: &str, loads: &[&str]) -> Vec<u8> {
    let mut commands = Vec::new();
    commands.extend(dylib_command(LC_ID_DYLIB, id));
    for lib in loads {
        commands.extend(dylib_command(LC_LOAD_DYLIB, lib));
    }
    build_with(MH_DYLIB, &commands, 1 + loads.len())
}

/// Build a minimal Mach-O 64-bit thin executable with `loads` as the
/// `LC_LOAD_DYLIB` dependencies. Executables carry no `LC_ID_DYLIB`, so
/// goblin leaves `libs[0]` as the literal placeholder `"self"`.
pub fn build_thin_executable(loads: &[&str]) -> Vec<u8> {
    let mut commands = Vec::new();
    for lib in loads {
        commands.extend(dylib_command(LC_LOAD_DYLIB, lib));
    }
    build_with(MH_EXECUTE, &commands, loads.len())
}

fn build_with(filetype: u32, commands: &[u8], ncmds: usize) -> Vec<u8> {
    #[allow(clippy::cast_possible_truncation)]
    let ncmds_u32 = ncmds as u32;
    #[allow(clippy::cast_possible_truncation)]
    let sizeofcmds = commands.len() as u32;

    let mut out = Vec::with_capacity(32 + commands.len());
    out.extend(MH_MAGIC_64.to_le_bytes());
    out.extend(CPU_TYPE_X86_64.to_le_bytes());
    out.extend(CPU_SUBTYPE_X86_64_ALL.to_le_bytes());
    out.extend(filetype.to_le_bytes());
    out.extend(ncmds_u32.to_le_bytes());
    out.extend(sizeofcmds.to_le_bytes());
    out.extend(0u32.to_le_bytes()); // flags
    out.extend(0u32.to_le_bytes()); // reserved
    out.write_all(commands).unwrap();
    out
}
