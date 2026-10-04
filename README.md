# harbor-cad

Local-first Rust CLI/worker and narrow Python MCP for scientific CAD jobs.
The authoritative scope is [implementation-spec.md](docs/implementation-spec.md).
The current implementation is **pre-qualification**: the CPU analytical-reference
workflow is executable; native/GPU adapters require separate qualification.
Process success is never a physical-validation claim.

## Run the implemented reference workflow

```sh
cargo build --locked --jobs 2
uv sync --locked
harbor-cad worker --state /absolute/private/state --profile profiles/ci.json
harbor-cad case init > case.json
harbor-cad case plan case.json > planned.json
```

`case plan` returns `{approval_digest, plan}`. Save `plan` as `plan.json`, then:

```sh
harbor-cad --socket /absolute/private/state/worker.sock job submit plan.json \
  --approve <approval_digest> --idempotency-key reference-001
harbor-cad --socket /absolute/private/state/worker.sock job status <job-id>
harbor-cad --socket /absolute/private/state/worker.sock results describe <job-id>
harbor-cad artifact export --state /absolute/private/state <job-id> ./portable
```

The CI profile uses explicitly weaker foreground execution and permits only
analytical references and bundle indexing. Native stages require systemd job
services and exact Nix-packaged sandbox/runtime paths. Host drivers and GPU
permissions are operator-owned; nothing installs drivers or uploads artifacts.

MCP uses the same worker, with bounded structured responses and durable job IDs:

```sh
HARBOR_CAD_SOCKET=/absolute/private/state/worker.sock \
  uv run --locked harbor-cad-mcp --profile all
```

Profiles: `cad`, `simulation`, `results`, `all`. There is no arbitrary shell,
Python evaluation, package installation or scientific-array response tool.

## Verification

```sh
cargo test --locked --jobs 2
cargo clippy --locked --all-targets --jobs 2 -- -D warnings
HARBOR_CAD_TEST_BINARY="$PWD/target/debug/harbor-cad" uv run --locked pytest -q
treefmt --config-file treefmt.toml
```

`doctor` is read-only inventory, not a GPU benchmark. `qualify` reports gate
status without promoting untested combinations. Native package outputs and
execution receipts must be qualified before A0/B1 can be declared complete.
