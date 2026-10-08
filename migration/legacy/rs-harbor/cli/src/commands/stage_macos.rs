//! `rs-harbor stage macos` — produce per-arch and universal Mach-O dist
//! layouts from cargo's per-target outputs, including dSYM bundles.
//!
//! Assumes the consumer compiled with `split-debuginfo = "packed"` so each
//! per-target build dir has a `<binary>.dSYM` next to the binary.

#![allow(clippy::needless_pass_by_value, clippy::unnecessary_trailing_comma)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Args;
use goblin::Object;
use goblin::mach::constants::cputype::{CPU_TYPE_ARM64, CPU_TYPE_X86, CPU_TYPE_X86_64, CpuType};
use goblin::mach::{Mach, SingleArch};

#[derive(Args, Debug)]
pub struct StageMacosArgs {
    /// Cargo binary name (required).
    #[arg(long)]
    pub binary: String,

    /// Extra dylib to copy/lipo (repeatable, e.g. `libsteam_api.dylib`).
    #[arg(long)]
    pub dylib: Vec<String>,

    /// Comma-separated list of architectures to combine.
    #[arg(long, default_value = "x86_64,aarch64")]
    pub archs: String,

    /// Cargo target dir.
    #[arg(long, default_value = "target")]
    pub target_dir: PathBuf,

    /// Dist root (per-arch and universal outputs land here).
    #[arg(long, default_value = "dist")]
    pub dist_dir: PathBuf,

    /// dSYM output subdirectory under `dist_dir`.
    #[arg(long, default_value = "symbols/macos")]
    pub symbols_subdir: PathBuf,

    /// Skip producing per-arch directories.
    #[arg(long)]
    pub skip_per_arch: bool,

    /// Skip producing the universal slice.
    #[arg(long)]
    pub skip_universal: bool,
}

pub fn run(args: StageMacosArgs) -> Result<()> {
    let archs = parse_archs(&args.archs)?;
    verify_inputs(&args, &archs)?;
    fs::create_dir_all(&args.dist_dir)
        .with_context(|| format!("creating dist dir {}", args.dist_dir.display()))?;

    if !args.skip_per_arch {
        stage_per_arch(&args, &archs)?;
    }

    if !args.skip_universal {
        // lipo is only needed for the universal slice; resolve it lazily
        // so callers using --skip-universal don't need it on PATH.
        let lipo = which_lipo()?;
        stage_universal(&args, &archs, &lipo)?;
    }

    println!(
        "rs-harbor stage macos: staged {} for archs {} into {}/",
        args.binary,
        args.archs,
        args.dist_dir.display(),
    );
    Ok(())
}

fn parse_archs(archs: &str) -> Result<Vec<String>> {
    let archs: Vec<String> = archs
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    if archs.is_empty() {
        bail!("--archs must list at least one arch");
    }
    Ok(archs)
}

fn src_for(target_dir: &Path, arch: &str) -> PathBuf {
    target_dir
        .join(format!("{arch}-apple-darwin"))
        .join("release")
}

fn dsym_name(binary: &str) -> String {
    format!("{binary}.dSYM")
}

fn verify_inputs(args: &StageMacosArgs, archs: &[String]) -> Result<()> {
    for arch in archs {
        let src = src_for(&args.target_dir, arch);
        let bin = src.join(&args.binary);
        let dsym = src.join(dsym_name(&args.binary));
        if !bin.is_file() {
            bail!("missing {}", bin.display());
        }
        if !dsym.is_dir() {
            bail!(
                "missing {} (build with split-debuginfo=\"packed\")",
                dsym.display()
            );
        }
    }
    Ok(())
}

fn stage_per_arch(args: &StageMacosArgs, archs: &[String]) -> Result<()> {
    for arch in archs {
        let src = src_for(&args.target_dir, arch);
        let out = args.dist_dir.join(format!("macos-{arch}"));
        fs::create_dir_all(&out)?;
        copy_file(&src.join(&args.binary), &out.join(&args.binary))?;
        for dylib in &args.dylib {
            let candidate = src.join(dylib);
            if candidate.is_file() {
                copy_file(&candidate, &out.join(dylib))?;
            }
        }
        let sym_out = args.dist_dir.join(&args.symbols_subdir).join(arch);
        fs::create_dir_all(&sym_out)?;
        copy_dir_recursive(
            &src.join(dsym_name(&args.binary)),
            &sym_out.join(dsym_name(&args.binary)),
        )?;
    }
    Ok(())
}

fn stage_universal(args: &StageMacosArgs, archs: &[String], lipo: &Path) -> Result<()> {
    let out = args.dist_dir.join("macos");
    fs::create_dir_all(&out)?;

    let bin_inputs: Vec<PathBuf> = archs
        .iter()
        .map(|a| src_for(&args.target_dir, a).join(&args.binary))
        .collect();
    lipo_or_copy(&bin_inputs, archs, lipo, &out.join(&args.binary))?;

    for dylib in &args.dylib {
        let dy_inputs: Vec<PathBuf> = archs
            .iter()
            .map(|a| src_for(&args.target_dir, a).join(dylib))
            .filter(|p| p.is_file())
            .collect();
        if dy_inputs.is_empty() {
            continue;
        }
        lipo_or_copy(&dy_inputs, archs, lipo, &out.join(dylib))?;
    }

    let sym_dst = args
        .dist_dir
        .join(&args.symbols_subdir)
        .join(dsym_name(&args.binary));
    let first_src = src_for(&args.target_dir, &archs[0]).join(dsym_name(&args.binary));
    if sym_dst.exists() {
        fs::remove_dir_all(&sym_dst)?;
    }
    if let Some(parent) = sym_dst.parent() {
        fs::create_dir_all(parent)?;
    }
    copy_dir_recursive(&first_src, &sym_dst)?;

    if archs.len() > 1 {
        let dwarf_inputs: Vec<PathBuf> = archs
            .iter()
            .map(|a| {
                let dsym = src_for(&args.target_dir, a).join(dsym_name(&args.binary));
                find_dwarf_file(&dsym, &args.binary)
            })
            .collect::<Result<Vec<_>>>()?;

        // Drop the per-arch DWARF file copied in from arch[0]'s dSYM and
        // emit the merged DWARF under the canonical unhashed name. Cargo
        // with split-debuginfo=packed names the per-arch DWARF
        // `{binary}-{rustc_metadata_hash}`, so the previous hardcoded
        // `{binary}` path would never have matched.
        let dwarf_subdir = sym_dst.join("Contents/Resources/DWARF");
        if dwarf_subdir.exists() {
            for entry in fs::read_dir(&dwarf_subdir)? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    fs::remove_file(entry.path())?;
                }
            }
        }
        lipo_or_copy(&dwarf_inputs, archs, lipo, &dwarf_subdir.join(&args.binary))?;
    }

    Ok(())
}

fn find_dwarf_file(dsym_dir: &Path, binary: &str) -> Result<PathBuf> {
    let dwarf_dir = dsym_dir.join("Contents/Resources/DWARF");
    let canonical = dwarf_dir.join(binary);
    if canonical.is_file() {
        return Ok(canonical);
    }
    let prefix = format!("{binary}-");
    let mut matches: Vec<PathBuf> = Vec::new();
    for entry in
        fs::read_dir(&dwarf_dir).with_context(|| format!("reading {}", dwarf_dir.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(&prefix) && entry.file_type()?.is_file() {
            matches.push(entry.path());
        }
    }
    match matches.len() {
        0 => bail!(
            "no DWARF file for binary {} under {}",
            binary,
            dwarf_dir.display()
        ),
        1 => Ok(matches.pop().unwrap()),
        n => bail!(
            "expected one DWARF file for binary {} under {}, found {}",
            binary,
            dwarf_dir.display(),
            n,
        ),
    }
}

/// Combine per-arch Mach-O inputs into a universal output.
///
/// If any single input already contains every requested arch (i.e. it is a
/// universal binary covering the whole target set), it is copied verbatim
/// rather than fed to lipo. This is required for SDK-provided fat dylibs
/// (e.g. Steamworks' `libsteam_api.dylib`), which `steamworks-sys` blindly
/// copies into every per-target build dir — lipo refuses to merge two
/// universal inputs that carry the same arch slices.
fn lipo_or_copy(
    inputs: &[PathBuf],
    requested_archs: &[String],
    lipo: &Path,
    output: &Path,
) -> Result<()> {
    if inputs.len() == 1 {
        copy_file(&inputs[0], output)?;
        return Ok(());
    }

    if let Some(needed) = requested_archs
        .iter()
        .map(|a| cargo_arch_to_cputype(a))
        .collect::<Option<Vec<CpuType>>>()
    {
        for input in inputs {
            if let Ok(archs) = read_cputypes(input)
                && needed.iter().all(|n| archs.contains(n))
            {
                copy_file(input, output)?;
                return Ok(());
            }
        }
    }

    run_lipo_create(lipo, inputs, output)
}

fn cargo_arch_to_cputype(arch: &str) -> Option<CpuType> {
    match arch {
        "x86_64" => Some(CPU_TYPE_X86_64),
        "aarch64" | "arm64" => Some(CPU_TYPE_ARM64),
        "i386" | "x86" => Some(CPU_TYPE_X86),
        _ => None,
    }
}

fn read_cputypes(path: &Path) -> Result<Vec<CpuType>> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    match Object::parse(&bytes) {
        Ok(Object::Mach(Mach::Binary(macho))) => Ok(vec![macho.header.cputype]),
        Ok(Object::Mach(Mach::Fat(fat))) => Ok(fat
            .into_iter()
            .filter_map(Result::ok)
            .filter_map(|slice| match slice {
                SingleArch::MachO(m) => Some(m.header.cputype),
                SingleArch::Archive(_) => None,
            })
            .collect()),
        _ => bail!("not a Mach-O file: {}", path.display()),
    }
}

fn which_lipo() -> Result<PathBuf> {
    for name in ["lipo", "llvm-lipo"] {
        if let Ok(path) = which::which(name) {
            return Ok(path);
        }
    }
    bail!("neither lipo nor llvm-lipo found in PATH")
}

fn run_lipo_create(lipo: &Path, inputs: &[PathBuf], output: &Path) -> Result<()> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut cmd = Command::new(lipo);
    cmd.arg("-create");
    for input in inputs {
        cmd.arg(input);
    }
    cmd.arg("-output").arg(output);
    let status = cmd
        .status()
        .with_context(|| format!("running {}", lipo.display()))?;
    if !status.success() {
        bail!("{} -create failed for {}", lipo.display(), output.display(),);
    }
    Ok(())
}

fn copy_file(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dst)
        .with_context(|| format!("copying {} -> {}", src.display(), dst.display()))?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    if !src.is_dir() {
        bail!("source is not a directory: {}", src.display());
    }
    if dst.exists() {
        fs::remove_dir_all(dst)?;
    }
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else if ty.is_symlink() {
            #[cfg(unix)]
            {
                let target = fs::read_link(entry.path())?;
                std::os::unix::fs::symlink(target, &dst_path)?;
            }
            #[cfg(not(unix))]
            {
                fs::copy(entry.path(), &dst_path)?;
            }
        } else {
            fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(flatten)]
        args: StageMacosArgs,
    }

    #[test]
    fn parses_minimal_args() {
        let cli = TestCli::try_parse_from(["stage-macos", "--binary", "myapp"]).expect("parses");
        assert_eq!(cli.args.binary, "myapp");
        assert_eq!(cli.args.archs, "x86_64,aarch64");
        assert!(cli.args.dylib.is_empty());
        assert!(!cli.args.skip_per_arch);
        assert!(!cli.args.skip_universal);
    }

    #[test]
    fn parses_repeated_dylibs_and_overrides() {
        let cli = TestCli::try_parse_from([
            "stage-macos",
            "--binary",
            "myapp",
            "--dylib",
            "libfoo.dylib",
            "--dylib",
            "libbar.dylib",
            "--archs",
            "aarch64",
            "--target-dir",
            "build/cargo",
            "--skip-per-arch",
        ])
        .expect("parses");
        assert_eq!(cli.args.dylib, vec!["libfoo.dylib", "libbar.dylib"]);
        assert_eq!(cli.args.archs, "aarch64");
        assert_eq!(cli.args.target_dir, PathBuf::from("build/cargo"));
        assert!(cli.args.skip_per_arch);
    }

    #[test]
    fn parse_archs_strips_whitespace_and_empties() {
        assert_eq!(
            parse_archs("x86_64, aarch64 ,").unwrap(),
            vec!["x86_64", "aarch64"]
        );
    }

    #[test]
    fn parse_archs_rejects_empty_list() {
        assert!(parse_archs(",,").is_err());
    }

    #[test]
    fn cargo_arch_to_cputype_known() {
        assert_eq!(cargo_arch_to_cputype("x86_64"), Some(CPU_TYPE_X86_64));
        assert_eq!(cargo_arch_to_cputype("aarch64"), Some(CPU_TYPE_ARM64));
        assert_eq!(cargo_arch_to_cputype("arm64"), Some(CPU_TYPE_ARM64));
        assert_eq!(cargo_arch_to_cputype("i386"), Some(CPU_TYPE_X86));
        assert_eq!(cargo_arch_to_cputype("powerpc"), None);
    }

    #[test]
    fn lipo_or_copy_uses_universal_input_verbatim() {
        // Fabricate a fat Mach-O with one x86_64 and one arm64 slice and
        // place it under both per-arch input paths. lipo_or_copy must pick
        // it up via cputype detection and copy verbatim — calling lipo on
        // two universal inputs that share an arch would error.
        let tmp = tempfile::tempdir().expect("tempdir");
        let fat_bytes = fabricate_fat_macho(&[CPU_TYPE_X86_64, CPU_TYPE_ARM64]);
        let x86 = tmp.path().join("x86.dylib");
        let arm = tmp.path().join("arm.dylib");
        let out = tmp.path().join("universal.dylib");
        fs::write(&x86, &fat_bytes).unwrap();
        fs::write(&arm, &fat_bytes).unwrap();

        let bogus_lipo = PathBuf::from("/does-not-exist/lipo");
        lipo_or_copy(
            &[x86.clone(), arm.clone()],
            &["x86_64".into(), "aarch64".into()],
            &bogus_lipo,
            &out,
        )
        .expect("verbatim copy path must not invoke lipo");
        assert_eq!(fs::read(&out).unwrap(), fat_bytes);
    }

    #[test]
    fn read_cputypes_reads_thin_macho() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let p = tmp.path().join("thin.dylib");
        fs::write(&p, fabricate_thin_macho(CPU_TYPE_X86_64)).unwrap();
        let archs = read_cputypes(&p).expect("parse");
        assert_eq!(archs, vec![CPU_TYPE_X86_64]);
    }

    #[test]
    fn find_dwarf_file_prefers_canonical_name() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dwarf_dir = tmp.path().join("foo.dSYM/Contents/Resources/DWARF");
        fs::create_dir_all(&dwarf_dir).unwrap();
        fs::write(dwarf_dir.join("foo"), b"x").unwrap();
        fs::write(dwarf_dir.join("foo-abcdef"), b"x").unwrap();
        let found = find_dwarf_file(&tmp.path().join("foo.dSYM"), "foo").unwrap();
        assert_eq!(found.file_name().unwrap(), "foo");
    }

    #[test]
    fn find_dwarf_file_falls_back_to_hashed_name() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dwarf_dir = tmp.path().join("foo.dSYM/Contents/Resources/DWARF");
        fs::create_dir_all(&dwarf_dir).unwrap();
        fs::write(dwarf_dir.join("foo-deadbeef"), b"x").unwrap();
        let found = find_dwarf_file(&tmp.path().join("foo.dSYM"), "foo").unwrap();
        assert_eq!(found.file_name().unwrap(), "foo-deadbeef");
    }

    #[test]
    fn find_dwarf_file_errors_when_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dwarf_dir = tmp.path().join("foo.dSYM/Contents/Resources/DWARF");
        fs::create_dir_all(&dwarf_dir).unwrap();
        let err = find_dwarf_file(&tmp.path().join("foo.dSYM"), "foo").unwrap_err();
        assert!(err.to_string().contains("no DWARF file"));
    }

    #[test]
    fn find_dwarf_file_errors_on_ambiguous_match() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dwarf_dir = tmp.path().join("foo.dSYM/Contents/Resources/DWARF");
        fs::create_dir_all(&dwarf_dir).unwrap();
        fs::write(dwarf_dir.join("foo-aaa"), b"x").unwrap();
        fs::write(dwarf_dir.join("foo-bbb"), b"x").unwrap();
        let err = find_dwarf_file(&tmp.path().join("foo.dSYM"), "foo").unwrap_err();
        assert!(err.to_string().contains("expected one DWARF file"));
    }

    #[test]
    fn read_cputypes_reads_fat_macho() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let p = tmp.path().join("fat.dylib");
        fs::write(&p, fabricate_fat_macho(&[CPU_TYPE_X86_64, CPU_TYPE_ARM64])).unwrap();
        let archs = read_cputypes(&p).expect("parse");
        assert!(archs.contains(&CPU_TYPE_X86_64));
        assert!(archs.contains(&CPU_TYPE_ARM64));
    }

    /// Build a minimal 64-bit thin Mach-O header (28 bytes) sufficient for
    /// goblin to identify the file and report its `cputype`.
    fn fabricate_thin_macho(cputype: CpuType) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&0xFEED_FACF_u32.to_le_bytes()); // MH_MAGIC_64
        v.extend_from_slice(&cputype.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes()); // cpusubtype
        v.extend_from_slice(&0u32.to_le_bytes()); // filetype (MH_OBJECT=1, 0 also accepted)
        v.extend_from_slice(&0u32.to_le_bytes()); // ncmds
        v.extend_from_slice(&0u32.to_le_bytes()); // sizeofcmds
        v.extend_from_slice(&0u32.to_le_bytes()); // flags
        v.extend_from_slice(&0u32.to_le_bytes()); // reserved (64-bit only)
        v
    }

    /// Build a minimal big-endian `FAT_MAGIC` Mach-O wrapping thin slices for
    /// each requested `cputype`. Layout: 8-byte fat header, then `nfat_arch`
    /// 20-byte `fat_arch` entries (big-endian), then the thin slices at the
    /// declared offsets.
    fn fabricate_fat_macho(cputypes: &[CpuType]) -> Vec<u8> {
        let header_size = 8 + cputypes.len() * 20;
        let slice = fabricate_thin_macho(CPU_TYPE_X86_64); // arch-agnostic shape
        let slice_size = slice.len();

        let mut v = Vec::new();
        v.extend_from_slice(&0xCAFE_BABE_u32.to_be_bytes()); // FAT_MAGIC
        v.extend_from_slice(&u32::try_from(cputypes.len()).unwrap().to_be_bytes());
        for (i, cpu) in cputypes.iter().enumerate() {
            let offset = u32::try_from(header_size + i * slice_size).unwrap();
            v.extend_from_slice(&cpu.to_be_bytes()); // cputype
            v.extend_from_slice(&0u32.to_be_bytes()); // cpusubtype
            v.extend_from_slice(&offset.to_be_bytes()); // offset
            v.extend_from_slice(&u32::try_from(slice_size).unwrap().to_be_bytes()); // size
            v.extend_from_slice(&0u32.to_be_bytes()); // align
        }
        for cpu in cputypes {
            v.extend_from_slice(&fabricate_thin_macho(*cpu));
        }
        v
    }
}
