{harbor-meta, ...}: rec {
  packageTests = harbor-meta.lib.packageTests;
  timezone = harbor-meta.lib.timezone;
  docs = import ./docs.nix {
    inherit packageTests;
    metaDevShell = harbor-meta.lib.devShell;
  };
  checks = import ./checks.nix {};

  inherit (docs) mkBookToml mkDocs mkSite mkDocsDevShell;
  inherit (checks) mkSummaryCheck mkWebsiteMarkers;
}
