# Equal-accuracy native CPU measurements

`scripts/verify_equal_accuracy_cpu.py` compares one- and two-core effective
execution profiles on one already qualified synthetic thermal case. Geometry,
material properties, physical histories, native step, observation times,
Float64 originals and the immutable scientific approval stay identical. It
requires matching exact production CLI/MCP/runtime paths and independently
reconstructs the complete standalone thermal reference/refinement campaign
before creating output state.

One retained warmup per configuration precedes five to ten paired repetitions.
The within-pair order alternates; every original, observation, failed attempt and
sample is retained without outlier trimming. The measured wall clock runs from
ordinary approved submission through atomic offline export and checksum
verification. Independent field replay is run separately for every observation
and is excluded from the timed interval. Every original retained temperature,
point ID and physical time must match the baseline within `1e-12 K`; each job
also passes the unchanged independent temperature/energy and historical original
byte-verification gates. Kernel CPU quota, no-swap controls, CPU usage, aggregate
RAM/task peaks and scientific/export byte counts are recorded.

The report gives median/minimum/maximum observations and the fastest observed
median execution profile for this fixed case/runtime/host. It does not change
dispatch defaults or generalize that result to a different solver size, host or
GPU. This precompiled CPU solver has no JIT; retained warmups do not imply a
controlled cold OS cache, and no VRAM measurement is inferred. Numerical equality
and recorded execution do not establish physical validation.

```sh
python scripts/verify_equal_accuracy_cpu.py \
  --executable /nix/store/CLI/bin/harbor-cad \
  --mcp /nix/store/MCP/bin/harbor-cad-mcp \
  --runtime /nix/store/THERMAL-WORKER.json \
  --authority /absolute/installed-authority.json \
  --native-reference /absolute/exact-thermal-native-gate \
  --output /absolute/fresh-short-path-benchmark
```

Run this opt-in workload through the normal Atlas runtime lease and a bounded
service. The worker owns every admission, service identity and lifetime;
successful samples must release their shared reservations and runtime roots
before changing the next effective profile.

The exact packaged 2026-10-08 campaign passed all 12 retained observations,
including two warmups. Five paired measured runs gave medians of `16.35355 s`
at one core and `15.45866 s` at two cores (ratio `0.94528`). All original
temperatures match exactly, with the same unchanged independent scientific
gates. Maximum observed job-cgroup RAM was `71,417,856` and `192,008,192` bytes,
respectively. [Checksummed evidence](evidence/equal-accuracy-cpu-20261008.json)
binds every original observation, execution profile, package and report. The
memory/time tradeoff and fixed-case measurement scope remain explicit.
