# harbor-ci

`harbor-ci` provides a small, reproducible Cargo quality gate for Rust
workspaces. It reads `[workspace.metadata.harbor-ci]` (or the equivalent
package metadata) from `Cargo.toml` and runs the selected profile:

```sh
harbor-ci fast
harbor-ci default
harbor-ci full --report target/harbor-ci.json
```

The metadata supports package selection and workspace exclusions:

```toml
[workspace.metadata.harbor-ci]
test-runner = "nextest"
exclude = ["integration-tests"]
nextest-args = ["--profile", "ci", "-E", "not binary(intheavy_*)"]
```

`exclude` is emitted as Cargo's repeated `--exclude` option for workspace Cargo
stages and cannot be combined with `packages`. The formatting stage remains
workspace-wide because rustfmt does not support Cargo package exclusions.
`nextest-args` is passed as individual arguments to the nextest stage and
requires `test-runner = "nextest"`; it is never parsed as shell text.

Project-specific feature matrices, service provisioning, and compile-only
checks should remain in the project's own CI command. `harbor-ci` is intended
to cover the uniform Cargo gate, not become a second workflow language.
