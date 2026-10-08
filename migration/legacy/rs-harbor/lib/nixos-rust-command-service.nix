{hardeningProfiles}: {
  pkgs,
  name,
  package,
  executable,
  args ? [],
  user ? name,
  group ? user,
  type ? "simple",
  wantedBy ? ["multi-user.target"],
  after ? ["network-online.target"],
  requires ? [],
  wants ? ["network-online.target"],
  hardening ? "network-service",
  restart ? (
    if type == "oneshot"
    then "no"
    else "on-failure"
  ),
  restartSec ? 5,
  tmpfiles ? [],
  configToml ? null,
  extraServiceConfig ? {},
}:
assert pkgs.lib.assertMsg (name != "")
"rs-harbor.mkRustCommandServiceModule: 'name' is required";
assert pkgs.lib.assertMsg (executable != "")
"rs-harbor.mkRustCommandServiceModule: 'executable' is required"; let
  inherit (pkgs.lib) optionalAttrs;
  profile =
    if builtins.isString hardening
    then hardeningProfiles.${hardening} or (throw "rs-harbor.mkRustCommandServiceModule: unknown hardening profile '${hardening}'")
    else hardening;
  baseServiceConfig = builtins.removeAttrs profile [
    "CapabilityBoundingSet"
    "RestrictAddressFamilies"
  ];
  binary =
    if pkgs.lib.hasPrefix "/" executable
    then executable
    else "${package}/${executable}";
  serviceConfig =
    baseServiceConfig
    // {
      ExecStart = pkgs.lib.escapeShellArgs ([binary] ++ args);
      User = user;
      Group = group;
      Restart = restart;
      RestartSec = restartSec;
      CapabilityBoundingSet = profile.CapabilityBoundingSet or "";
      RestrictAddressFamilies = profile.RestrictAddressFamilies or ["AF_INET" "AF_INET6"];
    }
    // extraServiceConfig;
in
  {
    users.users.${user} = {
      isSystemUser = true;
      group = group;
    };
    users.groups.${group} = {};
    systemd.services.${name} = {
      description = "${name} service";
      inherit after requires wants wantedBy;
      serviceConfig = serviceConfig // {Type = type;};
    };
  }
  // optionalAttrs (tmpfiles != []) {
    systemd.tmpfiles.rules = tmpfiles;
  }
  // optionalAttrs (configToml != null) {
    environment.etc."${name}/config.toml".text = configToml;
  }
