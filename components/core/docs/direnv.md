# nix-direnv

`packages.<system>.core-nix-direnv` is nixpkgs' nix-direnv with two patches.
`.envrc` files keep plain `use flake` / `use nix`; nothing new is spelled there.

- **Batched input roots.** Flake inputs are deduplicated and rooted in one
  store operation with source compilation disabled. A renewed cache is
  published only after its input and profile roots succeed, so a failed
  renewal keeps the previous cache, including under `use flake … || return 1`.
- **Builds outlive the activation.** When a user systemd manager is reachable,
  `print-dev-env` runs in a transient unit `nix-direnv-<id>`. Cancelling the
  activation (Ctrl-C, an editor or agent timeout, a process-group SIGKILL) only
  stops waiting: the unit finishes once and its profile link roots the
  closure, so automatic GC cannot discard the work. The next activation joins
  a still-running unit or renews from the rooted result without rebuilding.
  Progress is tailed from `.direnv/*-tmp-profile*.log`; inspect a running
  build with `systemctl --user status 'nix-direnv-*'`. Concurrent activations
  of one profile serialize on `.direnv/.realize-<id>.lock`. Without a user
  manager (macOS, containers, CI) the call stays inline.

The Harbor Home Manager module selects this package by default when
`programs.harbor.enable` is set. Other configurations set
`programs.direnv.nix-direnv.package` to it. Building the shell in a unit moves
evaluation memory out of the caller's cgroup; the build itself already ran in
nix-daemon.

`checks.<system>.core-nix-direnv-roots` covers the inline path and patch
composition. `core-nix-direnv-runtime` is a VM test that covers interrupted,
rooted, joined, cached, concurrent and fallback activations.
