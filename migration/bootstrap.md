# Bootstrap tool inputs

Harbor imports Simit and Plinth as pinned source inputs (`flake = false`).
Their consumer flakes depend on Harbor, so importing those flakes into Harbor
would bring predecessor repositories back into every consumer's lock graph.
Source-only inputs keep that dependency cycle out of the evaluated graph.

`nix/bootstrap.nix` builds the tools with `lib.rust.mkToolchain` and the shared
package sets. It keeps Simit's embedded action registry, release helper,
review assets, declared `.simit/workflows` templates, byte-sensitive test
fixtures, package documents and native test dependencies. The `ci` shell
provides this Simit package. The template contract uses Plinth's pinned source
and a native `plinth-project` build adapter.

The Simit source pin is the hosted-qualified migration generator. The Plinth
source pin is the one used by that qualified Simit revision. Changing either
pin requires updating `nix/bootstrap-contract.nix` and qualifying the tools
and templates before consumers adopt the new Harbor revision.

The current generator is Simit 0.20.0 at `54ae206d02f7`; it preserves the
qualified `90bed59acf23` cohort and adds native monorepo failure diagnostics
and the hosted Nix 2.35.2 installer selection. Its original-run generator
artifact is the authority for regenerated workflows. Earlier `90bed59`
qualification receipts retain their original scope and identities.
The paired crate
workflows build credential-free signed-tag archives and publish through an
independent default-branch workflow. Publisher enrollment requires the exact
qualified engine revision in `SIMIT_PUBLISHER_REVISION` and a publishing token
scoped to the protected `simit-crates-io` environment. Generating the workflows
does not enroll credentials or publish a release.

Harbor's `simit-crates-io` environment is provisioned with the publishing token
and a branch policy for `feat/harbor-monorepo-components`. Its configured engine
revision is `54ae206d02f7ad371bb206c337594615894cd43e`. Harbor Cache `0.1.1`
was published from signed tag `harbor-cache/v0.1.1` at `acfb79c`, with original
archive SHA-256 `ec04e2dae69d71856f62cca8e3e57b7f0b14e8efdce1d043fcc0b633b822367f`
and trusted-publisher run `38513415281`. Enrollment and that package release
do not establish qualification of the later integrated Harbor source or
publication of other packages. Refresh provider policy and exact engine
acceptance before a subsequent release.

The `bootstrap-inputs` check verifies both source-only declarations, their
exact revisions, and the absence of retired owned Codeberg transports in the
shared lock. It is included in the core component aggregate. Imported history
and archived predecessor source trees retain their original references.

Validate a bootstrap change with the core aggregate, the native `ci` shell,
template output evaluation on all four maintained systems, generated-workflow
drift, and exact-revision hosted monorepo qualification. Consumer cutover still
requires its own successful reconciliation and production qualification.
