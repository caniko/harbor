# Hardening profiles for NixOS systemd services.
#
# Each profile is a plain attrset of systemd serviceConfig directives suitable
# for merging into a systemd service definition. Profiles are additive —
# consumers can update with extra overrides or compose multiple profiles.
#
# Usage:
#   services.my-service.serviceConfig = harbor-rs.lib.hardeningProfiles.network-service // {
#     ExecStart = "...";
#   };
{
  network-service = {
    AmbientCapabilities = "";
    CapabilityBoundingSet = "CAP_NET_BIND_SERVICE";
    LockPersonality = true;
    MemoryDenyWriteExecute = true;
    NoNewPrivileges = true;
    PrivateTmp = true;
    ProtectClock = true;
    ProtectControlGroups = true;
    ProtectHome = true;
    ProtectHostname = true;
    ProtectKernelLogs = true;
    ProtectKernelModules = true;
    ProtectKernelTunables = true;
    ProtectProc = "invisible";
    ProtectSystem = "strict";
    RestrictAddressFamilies = ["AF_INET" "AF_INET6"];
    RestrictNamespaces = true;
    RestrictRealtime = true;
    SystemCallArchitectures = "native";
  };

  compute-worker = {
    AmbientCapabilities = "";
    CapabilityBoundingSet = "";
    IPAddressDeny = "any";
    LockPersonality = true;
    MemoryDenyWriteExecute = true;
    NoNewPrivileges = true;
    PrivateNetwork = true;
    PrivateTmp = true;
    ProtectClock = true;
    ProtectControlGroups = true;
    ProtectHome = true;
    ProtectHostname = true;
    ProtectKernelLogs = true;
    ProtectKernelModules = true;
    ProtectKernelTunables = true;
    ProtectProc = "invisible";
    ProtectSystem = "strict";
    RestrictAddressFamilies = "none";
    RestrictNamespaces = true;
    RestrictRealtime = true;
    SystemCallArchitectures = "native";
  };

  max = {
    AmbientCapabilities = "";
    CapabilityBoundingSet = "";
    LockPersonality = true;
    MemoryDenyWriteExecute = true;
    NoNewPrivileges = true;
    PrivateDevices = true;
    PrivateNetwork = true;
    PrivateTmp = true;
    PrivateUsers = true;
    ProtectClock = true;
    ProtectControlGroups = true;
    ProtectHome = true;
    ProtectHostname = true;
    ProtectKernelLogs = true;
    ProtectKernelModules = true;
    ProtectKernelTunables = true;
    ProtectProc = "invisible";
    ProtectSystem = "strict";
    RestrictAddressFamilies = "none";
    RestrictNamespaces = true;
    RestrictRealtime = true;
    RestrictSUIDSGID = true;
    SystemCallArchitectures = "native";
  };

  minimal = {
    AmbientCapabilities = "";
  };
}
