//! Minimal ELF64 shared object for host-independent dependency audit tests.
//!
//! A load segment maps the whole file, and a dynamic segment locates the string
//! table and one `DT_NEEDED` entry. The fixture is parsed, never executed.

pub fn build_shared_object() -> Vec<u8> {
    let strings = b"\0libharbor-fixture.so\0";
    let dynamic_offset = 64 + 2 * 56;
    let dynamic_size = 4 * 16;
    let strings_offset = dynamic_offset + dynamic_size;
    let file_size = strings_offset + u64::try_from(strings.len()).unwrap();
    let mut bytes = Vec::new();

    bytes.extend(b"\x7fELF\x02\x01\x01\0"); // ELF64, little-endian, current version.
    bytes.extend([0; 8]); // Remaining e_ident padding.
    bytes.extend(3u16.to_le_bytes()); // ET_DYN.
    bytes.extend(62u16.to_le_bytes()); // EM_X86_64; independent of the host CPU.
    bytes.extend(1u32.to_le_bytes()); // EV_CURRENT.
    bytes.extend(0u64.to_le_bytes()); // Entry address.
    bytes.extend(64u64.to_le_bytes()); // Program header offset.
    bytes.extend(0u64.to_le_bytes()); // No section headers.
    bytes.extend(0u32.to_le_bytes()); // Flags.
    for field in [64u16, 56, 2, 64, 0, 0] {
        bytes.extend(field.to_le_bytes());
    }

    program_header(&mut bytes, 1, 0, file_size, 0x1000); // PT_LOAD.
    program_header(&mut bytes, 2, dynamic_offset, dynamic_size, 8); // PT_DYNAMIC.
    for (tag, value) in [
        (5u64, strings_offset), // DT_STRTAB.
        (10, u64::try_from(strings.len()).unwrap()), // DT_STRSZ.
        (1, 1), // DT_NEEDED: libharbor-fixture.so.
        (0, 0), // DT_NULL.
    ] {
        bytes.extend(tag.to_le_bytes());
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(strings);
    bytes
}

fn program_header(bytes: &mut Vec<u8>, kind: u32, offset: u64, size: u64, align: u64) {
    bytes.extend(kind.to_le_bytes());
    bytes.extend(4u32.to_le_bytes()); // PF_R.
    for field in [offset, offset, offset, size, size, align] {
        bytes.extend(field.to_le_bytes());
    }
}
