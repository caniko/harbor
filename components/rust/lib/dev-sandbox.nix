# A realized tool environment is reused for every application edit. Runtime does
# not evaluate Nix, inherit a host devShell, or call another fleet frontend.
{
  pkgs,
  harborRsCli,
  name ? "dev-sandbox",
  devShell ? null,
  environment ? null,
  inputs,
  command,
  services ? [],
  build ? null,
  ready ? null,
  designCopies ? [],
  desktop ? "none",
  network ? false,
  gpu ? false,
  cargoConfig ? null,
  pollMs ? 200,
  debounceMs ? 300,
  buildTimeoutSeconds ? 300,
  readyTimeoutSeconds ? 15,
}: let
  inherit (pkgs) lib;
  spec =
    if environment != null
    then environment
    else if devShell != null && devShell ? devShellSpec
    then devShell.devShellSpec
    else if devShell != null && devShell ? passthru && devShell.passthru ? devShellSpec
    then devShell.passthru.devShellSpec
    else throw "harbor-rs mkDevSandbox: supply environment or a Harbor devShell";
  shellPackages =
    if devShell == null
    then []
    else (devShell.nativeBuildInputs or []) ++ (devShell.buildInputs or []);
  packages =
    (spec.packages or [])
    ++ shellPackages
    ++ [pkgs.bash pkgs.coreutils pkgs.findutils pkgs.gnugrep]
    ++ lib.optionals (desktop != "none") [pkgs.sway pkgs.dbus pkgs.mesa];
  profileData = {
    schemaVersion = 1;
    inherit name inputs command services build ready designCopies desktop network gpu pollMs debounceMs buildTimeoutSeconds readyTimeoutSeconds;
    tools = {
      bwrap = lib.getExe pkgs.bubblewrap;
      bash = lib.getExe pkgs.bash;
      dbus =
        if desktop == "none"
        then null
        else "${pkgs.dbus}/bin/dbus-run-session";
      sway =
        if desktop == "none"
        then null
        else lib.getExe pkgs.sway;
    };
    cargoConfig =
      if cargoConfig == null
      then null
      else toString cargoConfig.configPath;
    environment =
      lib.mapAttrs (_: toString) (spec.env or {})
      // {
        PATH = lib.makeBinPath packages;
        XDG_DATA_DIRS = lib.makeSearchPath "share" packages;
      };
    shellHook = spec.shellHook or "";
  };
  profile = pkgs.writeText "${name}-profile.json" (builtins.toJSON profileData);
in
  assert lib.assertMsg pkgs.stdenv.hostPlatform.isLinux "harbor-rs mkDevSandbox currently requires Linux";
  assert lib.assertMsg (builtins.elem desktop ["none" "headless" "nested"]) "unknown sandbox desktop profile";
    pkgs.writeShellApplication {
      inherit name;
      text = ''
        exec ${lib.getExe harborRsCli} sandbox watch --profile ${profile} "$@"
      '';
      derivationArgs.passthru = {inherit profile profileData;};
    }
