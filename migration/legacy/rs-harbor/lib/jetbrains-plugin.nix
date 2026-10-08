# mkJetBrainsPlugin :: Gradle package arguments + pluginXmlId -> derivation
#
# JetBrains packaging is intentionally a thin policy layer over mkGradlePackage.
# It records the plugin identity for downstream release tooling, while signing
# and Marketplace publication remain consumer-owned CI concerns.
args @ {
  pkgs,
  pluginXmlId,
  ...
}: let
  package = import ./gradle-package.nix (builtins.removeAttrs args ["pluginXmlId"]);
in
  assert builtins.isString pluginXmlId
  && pluginXmlId != ""
  || throw "mkJetBrainsPlugin: pluginXmlId must be a non-empty string";
    package
    // {
      rsHarbor =
        package.rsHarbor
        // {
          helper = "mkJetBrainsPlugin";
          inherit pluginXmlId;
        };
      passthru =
        package.passthru
        // {
          rsHarbor =
            package.passthru.rsHarbor
            // {
              helper = "mkJetBrainsPlugin";
              inherit pluginXmlId;
            };
        };
    }
