# mkRustServiceModule :: {
#   lib ?, pkgs ?, name, binary, args,
#   configToml ?, hardening ? "network-service",
#   extraCapabilities ? [], extraAddressFamilies ? [],
#   tmpfiles ? [], extraServiceConfig ? {}
# } -> NixOS module attrs
#
# Generate a hardened NixOS systemd service for a Rust binary.
# Returns { users, groups, systemd, environment.etc } attrs
# suitable for merging into config.
{hardeningProfiles}: {
  pkgs ? null,
  lib ? pkgs.lib,
  name,
  binary,
  args,
  configToml ? null,
  hardening ? "network-service",
  extraCapabilities ? [],
  extraAddressFamilies ? [],
  tmpfiles ? [],
  extraServiceConfig ? {},
}: let
  inherit (builtins) filter concatStringsSep;
  assertMsg = condition: message:
    if condition
    then true
    else throw message;

  profile =
    if builtins.isString hardening
    then hardeningProfiles.${hardening} or (throw "harbor-rs.mkRustServiceModule: unknown hardening profile '${hardening}'")
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
      ExecStart =
        if binary == ""
        then throw "harbor-rs.mkRustServiceModule: 'binary' is required"
        else if args == ""
        then throw "harbor-rs.mkRustServiceModule: 'args' is required"
        else "${binary} ${args}";
      Restart = "on-failure";
      RestartSec = 5;
      User = name;
      Group = name;
      CapabilityBoundingSet = capabilityBoundingSet;
      RestrictAddressFamilies = addressFamilies;
    }
    // extraServiceConfig;
in
  assert assertMsg (name != "")
  "harbor-rs.mkRustServiceModule: 'name' is required"; {
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
    systemd.tmpfiles.rules = lib.mkIf (tmpfiles != []) tmpfiles;
    environment.etc."${name}/config.toml".text = lib.mkIf (configToml != null) configToml;
  }
