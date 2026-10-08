{harbor-meta, ...}: rec {
  packageTests = harbor-meta.lib.packageTests;
  docs = import ./docs.nix {inherit packageTests;};
  checks = import ./checks.nix {};

  inherit (docs) mkBookToml mkDocs mkSite mkDocsDevShell;
  inherit (checks) mkSummaryCheck mkWebsiteMarkers;
}
