# Development sandbox and design previews

`mkDevSandbox` packages a prepared environment and trusted application profile.
The Rust `harbor-sandbox` library owns its Linux runtime; `harbor-rs sandbox`
exposes `run`, `watch`, `status` and `stop`. Application edits do not reevaluate
Nix or rebuild the environment package.

```nix
designSandbox = harbor-rs.lib.mkDevSandbox {
  inherit pkgs;
  harborRsCli = harbor-rs.packages.${system}.harbor-rs;
  name = "design-preview";
  devShell = devShells.default;
  cargoConfig = toolchain.cargoConfig;
  inputs = ["Cargo.toml" "Cargo.lock" "crates" "assets"];
  build = ["cargo" "build" "--offline" "--locked" "-p" "preview"];
  command = ["/harbor/build/target/debug/preview"];
  ready = ["bash" "-c" ''test -S "$XDG_RUNTIME_DIR/preview.sock"''];
};
```

The readiness path must match the application's private socket contract. Commands
are argument arrays, not shell strings. Use an explicit `bash -c` only when the
application profile requires shell expansion. `environment = { packages; env;
shellHook; }` can replace `devShell` for other prepared environments. When using a
Harbor devShell, its native/build inputs are also included in the tool PATH.

```sh
design-preview --checkout /path/to/project --state-root /path/to/private-state --variant plush
harbor-rs sandbox status --session /path/printed/by/launcher
harbor-rs sandbox stop --session /path/printed/by/launcher
```

For one preview use `harbor-rs sandbox run --profile PROFILE --checkout PROJECT
--state-root STATE --variant NAME`. The packaged launcher defaults to `watch`.

## Boundaries

- Bubblewrap is required; there is no unconfined fallback. Each build/preview has
  a private mount, PID, IPC, UTS, user and (by default) network namespace.
- `/nix/store` is read-only. Only the declared source inputs are copied. Preview
  source is read-only; builds can generate files in their disposable source copy.
  Symlinks/special files and overlapping inputs are rejected. Git state, direnv
  state and existing targets cannot be source inputs.
- Original source and lockfiles are untouched. A build that changes a copied
  `Cargo.lock` or `flake.lock` fails qualification before preview replacement.
- HOME, XDG directories, temp files, Cargo state and artifacts are established
  before the shell hook runs. Parent environment variables, credentials, agent
  sockets, desktop buses and browser profiles are not inherited.
- The writable mounts are `/harbor/build` and `/harbor/state`. Build state is
  namespaced by profile/environment hash; runtime state is per named variant.
  Runtime sockets and artifacts additionally have generation-specific directories.
  Supervisor locks/control files stay outside the application's mounts.
- `network = true` shares the host network explicitly, including host localhost.
  Default offline Cargo requires project-vendored dependencies or a cache already
  populated in this variant. Do not put credentials in the profile's environment.
- `gpu = true` exposes `/dev/dri`. `desktop = "headless"` creates private Sway
  and D-Bus sessions with a software renderer. `desktop = "nested"` additionally
  requires the caller's current Wayland socket. It uses software rendering by
  default (SHM); `gpu = true` opts into GLES2, which requires a parent compositor
  capable of sharing its DRM render FD. It launches Sway
  nested inside that display, then points the application at the nested socket.
- Profiles and shell hooks are trusted executable configuration. Confinement
  limits accidental application writes and host access; this is not a hostile
  kernel-code isolation service. Resource admission belongs to the host/runner.

## Fast iteration

Declared input content and executable bits drive watch invalidation. Edits are
debounced, builds are serialized, and the previous namespace remains alive during
build and candidate readiness. Failed compilation or readiness leaves the last
working preview in place. Build and readiness waits are bounded. A new successful
candidate replaces the previous namespace only after its readiness probe succeeds.
Applications must use `$XDG_RUNTIME_DIR` for sockets to avoid fixed-path collisions.

The same named variant retains HOME/XDG data and incremental targets across runs.
Use different variant names for independent application configurations/results.
Generations and receipts are retained. Stop acts through the owning supervisor's
kernel lock and an exact run token; it never signals a stored PID or removes a
lock anchor. Signals to the CLI set the same cancellation path. Namespace exit
contains session children.

## Blender and browser adapters

`designCopies = ["blender/fixture.blend"]` selects declared input files to copy
under `$HARBOR_ARTIFACTS/designs/`. A Blender command can open that copy with
private HOME/XDG preferences and save exports to `$HARBOR_ARTIFACTS`. It cannot
write the original authoring file. Blender bridge registry paths, addon settings,
instance claims and export correctness remain adapter/application contracts.

Browser adapters use `$HARBOR_BROWSER_PROFILE` for generation-private profiles;
Blender receives a generation-private `BLENDER_USER_CONFIG`. These let a candidate
start while the previous preview remains alive. Firefox/native messaging and accessibility services use the private HOME/XDG and
D-Bus session. Include their packages and explicit service search paths in the
prepared environment. Configure mock providers and synthetic sensing targets in
the project profile. Merely installing test tools does not enable passive sensing.

Declare mock/helper services as `services = [["mock-provider" "--unix-socket"]]`.
They start in the candidate's private namespace and bus before the preview. The
readiness probe must check the services it needs. Each replacement gets new
services and private runtime sockets; namespace teardown contains their children.

## Evidence and validation

`receipt.json` records source/environment hashes, current and preview generation,
capabilities, phase, build/launch milliseconds, successful switches and last error.
Each generation retains source, build/preview logs and its artifact directory.
`status` checks the kernel lock, so stale receipts after crashes report interrupted.

Native tests:

```sh
cargo test --locked -p harbor-sandbox
# Prepared Harbor shell exports explicit HARBOR_TEST_* tool paths:
cargo test --locked -p harbor-sandbox --test runtime --test nested -- --ignored --test-threads=1
```

The hosted `sandbox-runtime` required gate runs actual namespace acceptance.
On ephemeral Ubuntu runners its job-scoped setup permits unprivileged user
namespaces required by the Nix-store Bubblewrap executable; no deployed host
configuration is altered. This setup belongs only to the sandbox gate.
Nested desktop, browser, accessibility, renderer and live Blender acceptance must
also exercise the consuming application's exact profile on its target host.
