# Hosted Linux namespace fixtures

The database inode-alias tests run both engines behind real bind mounts in an
unprivileged user/mount namespace. Both Linux lanes of Harbor run
`38362465603` failed before the fixture executed: `unshare` could not write
`/proc/self/uid_map` (`Operation not permitted`). Native Atlas qualification
had exercised the same assertions successfully.

Ubuntu 24.04 restricts unprivileged user namespaces through AppArmor.
[Ubuntu's release notes](https://discourse.ubuntu.com/t/ubuntu-24-04-lts-noble-numbat-release-notes/39890)
document the per-application `flags=(unconfined)` profile with the `userns,`
permission. Harbor's Nix-store `unshare` executable needs that permission on
the ephemeral hosted runner. The profile attaches only to
`/nix/store/*-util-linux-*/bin/unshare`.

The Simit-owned Rust and database lanes run `scripts/ci_userns.py` before
their tests. It first exercises UID mapping and an actual bind mount. If that
probe fails and AppArmor's namespace restriction is active, only a
GitHub-hosted runner may load the profile into its kernel. The script verifies
that the global restriction remains enabled and repeats the probe. A denied
map, mount, profile load, or unexpected global-policy change fails the lane.
The [AppArmor documentation](https://documentation.ubuntu.com/server/how-to/security/apparmor/)
describes loading profiles with `apparmor_parser`.

The sandbox runtime lane also enters through the admitted util-linux `unshare`
profile before launching its Bubblewrap fixtures. The Rust shell is prepared
first, then the fixture process receives its own user and mount namespace. This
replaces the older lane's global sysctl override while retaining all real
namespace, isolation and nested-desktop acceptance tests. The global restriction
stays enabled for the rest of the ephemeral runner.

For local capability diagnosis, run `python3 scripts/ci_userns.py --check` in
the approved Harbor shell. This mode only probes the namespace. The Darwin
lane retains the portable workspace scope. Database test sources and their
assertions remain identical to the retained qualification source.
