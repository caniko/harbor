# mkRustServiceModule :: {
#   pkgs, name, binary, args,
#   configToml ?, hardening ? "network-service",
#   extraCapabilities ? [], extraAddressFamilies ? [],
#   tmpfiles ? [], extraServiceConfig ? {}
# } -> NixOS module attrs
#
# Generate a hardened NixOS systemd service for a Rust binary.
# Returns { users, groups, systemd, environment.etc } attrs
# suitable for merging into config.
{hardeningProfiles}: {
  pkgs,
  name,
  binary,
  args,
  configToml ? null,
  hardening ? "network-service",
  extraCapabilities ? [],
  extraAddressFamilies ? [],
  tmpfiles ? [],
  extraServiceConfig ? {},
}:
assert pkgs.lib.assertMsg (name != "")
"rs-harbor.mkRustServiceModule: 'name' is required";
assert pkgs.lib.assertMsg (binary != "")
"rs-harbor.mkRustServiceModule: 'binary' is required";
assert pkgs.lib.assertMsg (args != "")
"rs-harbor.mkRustServiceModule: 'args' is required"; let
  inherit (builtins) filter concatStringsSep;
  inherit (pkgs.lib) optionalAttrs;

  profile =
    if builtins.isString hardening
    then hardeningProfiles.${hardening} or (throw "rs-harbor.mkRustServiceModule: unknown hardening profile '${hardening}'")
    else hardening;

  caps =
    if profile.CapabilityBoundingSet or "" == ""
    then extraCapabilities
    else [profile.CapabilityBoundingSet] ++ extraCapabilities;

  addressFamilies =
    if extraAddressFamilies != []
    then extraAddressFamilies
    else profile.RestrictAddressFamilies or ["AF_INET" "AF_INET6"];

  capabilityBoundingSet = concatStringsSep " " (filter (s: s != "") caps);

  baseServiceConfig = builtins.removeAttrs profile [
    "CapabilityBoundingSet"
    "RestrictAddressFamilies"
  ];

  serviceConfig =
    baseServiceConfig
    // {
      ExecStart = "${binary} ${args}";
      Restart = "on-failure";
      RestartSec = 5;
      User = name;
      Group = name;
      CapabilityBoundingSet = capabilityBoundingSet;
      RestrictAddressFamilies = addressFamilies;
    }
    // extraServiceConfig;
in
  {
    users.users.${name} = {
      isSystemUser = true;
      group = name;
    };
    users.groups.${name} = {};

    systemd.services.${name} = {
      description = "${name} service";
      after = ["network-online.target"];
      wants = ["network-online.target"];
      wantedBy = ["multi-user.target"];
      inherit serviceConfig;
    };
  }
  // optionalAttrs (tmpfiles != []) {
    systemd.tmpfiles.rules = tmpfiles;
  }
  // optionalAttrs (configToml != null) {
    environment.etc."${name}/config.toml".text = configToml;
  }
