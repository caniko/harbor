"""Small executable ELF fixtures; no compiler or downloaded application required."""

import os
import struct
import sys
from pathlib import Path

root = Path(sys.argv[1])
system = sys.argv[2]
machine, code = {
    "x86_64-linux": (62, bytes.fromhex("b83c00000031ff0f05")),
    "aarch64-linux": (183, bytes.fromhex("000080d2a80b80d2010000d4")),
}[system]
base = 0x400000
ident = b"\x7fELF\x02\x01\x01" + bytes(9)
header = struct.pack(
    "<16sHHIQQQIHHHHHH", ident, 2, machine, 1, base + 120, 64, 0, 0, 64, 56, 1, 0, 0, 0
)
segment = struct.pack(
    "<IIQQQQQQ", 1, 5, 0, base, base, 120 + len(code), 120 + len(code), 0x1000
)
payload = header + segment + code


def write(name, data, mode=0o755):
    path = root / name
    path.write_bytes(data)
    path.chmod(mode)


write("fixture", payload)
pie_header = struct.pack(
    "<16sHHIQQQIHHHHHH", ident, 3, machine, 1, 120, 64, 0, 0, 64, 56, 1, 0, 0, 0
)
pie_segment = struct.pack(
    "<IIQQQQQQ", 1, 5, 0, 0, 0, 120 + len(code), 120 + len(code), 0x1000
)
write("static-pie", pie_header + pie_segment + code)
for name, elf_type in [("relocatable", 1), ("core", 4)]:
    invalid = bytearray(payload)
    struct.pack_into("<H", invalid, 16, elf_type)
    write(name, invalid)
shared = bytearray(pie_header + pie_segment + code)
struct.pack_into("<Q", shared, 24, 0)
write("shared-library", shared)
for name, offset, value in [("no-load", 64, 4), ("non-executable-load", 68, 4)]:
    invalid = bytearray(payload)
    struct.pack_into("<I", invalid, offset, value)
    write(name, invalid)
outside = bytearray(payload)
struct.pack_into("<Q", outside, 24, base + len(payload))
write("unmapped-entry", outside)
write("non-executable", payload, 0o644)
write("script", b"#!/bin/sh\nexit 0\n")
wrong = bytearray(payload)
struct.pack_into("<H", wrong, 18, 183 if machine == 62 else 62)
write("wrong-machine", wrong)
# A second program header declares an interpreter, which static consumers reject.
dynamic = bytearray(header)
struct.pack_into("<H", dynamic, 56, 2)
dynamic += segment + struct.pack("<IIQQQQQQ", 3, 4, 176, 0, 0, 1, 1, 1)
dynamic += b"\0"
write("dynamic", dynamic)
needed = bytearray(header)
struct.pack_into("<H", needed, 56, 2)
needed += struct.pack("<IIQQQQQQ", 1, 5, 0, base, base, 208, 208, 0x1000)
needed += struct.pack("<IIQQQQQQ", 2, 4, 176, base + 176, base + 176, 32, 32, 8)
needed += struct.pack("<qQqQ", 1, 0, 0, 0)
write("dynamic-needed", needed)
os.symlink("fixture", root / "link")
os.symlink(".", root / "linked")
