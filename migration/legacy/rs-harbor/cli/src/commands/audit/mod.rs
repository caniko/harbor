//! `rs-harbor audit` — check that release-staged binaries only depend on
//! libraries available in the target deployment environment.
//!
//! Three flavours match the three native binary formats: `elf` (Linux,
//! e.g. Steam Runtime sniper), `pe` (Windows), `macho` (macOS).

#![allow(clippy::doc_markdown)]

use clap::Subcommand;

pub mod common;
pub mod elf;
pub mod macho;
pub mod pe;

#[derive(Subcommand, Debug)]
pub enum AuditCommand {
    /// Audit ELF executables/shared objects for forbidden RPATHs and
    /// disallowed DT_NEEDED entries (Linux / Steam Runtime).
    Elf(elf::AuditElfArgs),
    /// Audit PE executables for disallowed imported DLLs and forbidden
    /// embedded path strings (Windows).
    Pe(pe::AuditPeArgs),
    /// Audit Mach-O binaries for disallowed dylib dependencies (macOS).
    Macho(macho::AuditMachoArgs),
}

pub fn run(cmd: AuditCommand) -> anyhow::Result<()> {
    match cmd {
        AuditCommand::Elf(args) => elf::run(args),
        AuditCommand::Pe(args) => pe::run(args),
        AuditCommand::Macho(args) => macho::run(args),
    }
}
