//! Effective kernel controls and aggregate cgroup peaks for one owned service.
use crate::{Result, contracts::invalid, storage::safe_path, worker::read_bounded};
use std::{collections::BTreeMap, path::Path};

fn value(root: &Path, name: &str) -> Result<String> {
    String::from_utf8(read_bounded(&safe_path(root, name)?, 4096)?)
        .map(|value| value.trim().to_owned())
        .map_err(|_| invalid("UTF-8 cgroup counter required"))
}
fn counter(root: &Path, name: &str) -> Result<u64> {
    value(root, name)?
        .parse()
        .map_err(|_| invalid("finite unsigned cgroup counter required"))
}

pub(crate) fn capture(
    root: &Path,
    ram_bytes: u64,
    threads: u32,
    phase: &str,
) -> Result<serde_json::Value> {
    let controls: BTreeMap<_, _> = ["memory.max", "memory.swap.max", "cpu.max", "pids.max"]
        .into_iter()
        .map(|name| Ok((name, value(root, name)?)))
        .collect::<Result<_>>()?;
    let memory: u64 = controls["memory.max"]
        .parse()
        .map_err(|_| invalid("finite effective service RAM limit required"))?;
    let tasks: u64 = controls["pids.max"]
        .parse()
        .map_err(|_| invalid("finite effective service task limit required"))?;
    let cpu: Vec<u64> = controls["cpu.max"]
        .split_whitespace()
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| invalid("finite service CPU quota required"))?;
    if memory == 0
        || memory > ram_bytes
        || controls["memory.swap.max"] != "0"
        || tasks == 0
        || tasks > 128
        || cpu.len() != 2
        || cpu[0] == 0
        || cpu[1] == 0
        || threads == 0
        || cpu[0]
            > cpu[1]
                .checked_mul(u64::from(threads))
                .ok_or_else(|| invalid("service CPU quota overflow"))?
    {
        return Err(invalid(
            "effective owned-service limits exceed approved resources",
        ));
    }
    let mut cpu_stat = BTreeMap::new();
    for line in value(root, "cpu.stat")?.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 2
            || fields[0].len() > 64
            || cpu_stat
                .insert(
                    fields[0].to_owned(),
                    fields[1]
                        .parse::<u64>()
                        .map_err(|_| invalid("numeric service CPU statistic required"))?,
                )
                .is_some()
        {
            return Err(invalid("unique bounded cgroup CPU statistics required"));
        }
    }
    if !cpu_stat.contains_key("usage_usec") {
        return Err(invalid("aggregate CPU usage counter required"));
    }
    let pids_peak = match counter(root, "pids.peak") {
        Ok(value) => Some(value),
        Err(crate::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    Ok(
        serde_json::json!({"schema_version":1,"phase":phase,"controls":controls,
        "aggregate_memory_peak_bytes":counter(root,"memory.peak")?,"aggregate_memory_current_bytes":counter(root,"memory.current")?,
        "cpu_stat_microseconds_and_counts":cpu_stat,"aggregate_task_peak":pids_peak,
        "scope":"owned job service cgroup, including native descendants; lifetime peak through this capture",
        "vram":"not measured by cgroup counters"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_limits_and_peaks_cannot_be_invented_or_weakened() {
        let temp = tempfile::tempdir().unwrap();
        let files = [
            ("memory.max", "1073741824"),
            ("memory.swap.max", "0"),
            ("cpu.max", "100000 100000"),
            ("pids.max", "128"),
            ("memory.peak", "543210"),
            ("memory.current", "234567"),
            (
                "cpu.stat",
                "usage_usec 999\nuser_usec 900\nsystem_usec 99\n",
            ),
        ];
        for (name, value) in files {
            std::fs::write(temp.path().join(name), value).unwrap();
        }
        let result = capture(temp.path(), 1073741824, 1, "after_native_execution").unwrap();
        assert_eq!(result["aggregate_memory_peak_bytes"], 543210);
        assert_eq!(
            result["cpu_stat_microseconds_and_counts"]["usage_usec"],
            999
        );
        assert!(result["aggregate_task_peak"].is_null());
        for (name, value) in [
            ("memory.max", "max"),
            ("memory.max", "2147483648"),
            ("memory.swap.max", "1"),
            ("pids.max", "129"),
            ("cpu.max", "max 100000"),
            ("cpu.max", "200000 100000"),
            ("cpu.max", "0 0"),
            ("cpu.stat", "usage_usec 1\nusage_usec 2"),
            ("memory.peak", "unknown"),
        ] {
            let original = std::fs::read(temp.path().join(name)).unwrap();
            std::fs::write(temp.path().join(name), value).unwrap();
            assert!(capture(temp.path(), 1073741824, 1, "before_native_launch").is_err());
            std::fs::write(temp.path().join(name), original).unwrap();
        }
        std::os::unix::fs::symlink("memory.peak", temp.path().join("pids.peak")).unwrap();
        assert!(capture(temp.path(), 1073741824, 1, "after_native_execution").is_err());
    }
}
