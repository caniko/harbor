# Bootstrap tool inputs

Harbor imports Simit and Plinth as pinned source inputs (`flake = false`).
Their consumer flakes depend on Harbor, so importing those flakes into Harbor
would bring predecessor repositories back into every consumer's lock graph.
Source-only inputs keep that dependency cycle out of the evaluated graph.

`nix/bootstrap.nix` builds the tools with `lib.rust.mkToolchain` and the shared
package sets. It keeps Simit's embedded action registry, release helper,
review assets, workflow fixtures and native test dependencies. The `ci` shell
provides this Simit package. The template contract uses Plinth's pinned source
and a native `plinth-project` build adapter.

The Simit source pin is the hosted-qualified migration generator. The Plinth
source pin is the one used by that qualified Simit revision. Changing either
pin requires updating `nix/bootstrap-contract.nix` and qualifying the tools
and templates before consumers adopt the new Harbor revision.

The `bootstrap-inputs` check verifies both source-only declarations, their
exact revisions, and the absence of retired owned Codeberg transports in the
shared lock. It is included in the core component aggregate. Imported history
and archived predecessor source trees retain their original references.

Validate a bootstrap change with the core aggregate, the native `ci` shell,
template output evaluation on all four maintained systems, generated-workflow
drift, and exact-revision hosted monorepo qualification. Consumer cutover still
requires its own successful reconciliation and production qualification.
