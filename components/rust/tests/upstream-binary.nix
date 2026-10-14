{
  pkgs,
  harborLib,
}: let
  inherit (pkgs) lib;
  system = pkgs.stdenv.hostPlatform.system;
  fixture =
    pkgs.runCommand "upstream-static-binary-fixtures" {
      nativeBuildInputs = [pkgs.python3 pkgs.gnutar pkgs.gzip pkgs.xz];
    } ''
      mkdir -p "$out" release
      python3 ${./upstream-binary-fixtures.py} release ${lib.escapeShellArg system}
      tar -czf "$out/fixture.tar.gz" release
      tar -cJf "$out/fixture.tar.xz" release
      for target in x86_64-linux aarch64-linux; do
        mkdir "$target"
        python3 ${./upstream-binary-fixtures.py} "$target" "$target"
        tar -czf "$out/$target.tar.gz" "$target"
      done
    '';
  args = {
    pname = "upstream-fixture";
    version = "1";
    sources.${system} = "${fixture}/fixture.tar.gz";
    binaries.fixture = "release/fixture";
  };
  package = harborLib.mkUpstreamBinaryPackage (args // {inherit pkgs;});
  xzPackage = harborLib.mkUpstreamBinaryPackage (args
    // {
      inherit pkgs;
      sources.${system} = "${fixture}/fixture.tar.xz";
    });
  modePackage = harborLib.mkUpstreamBinaryPackage (args
    // {
      inherit pkgs;
      requireExecutable = false;
      binaries.fixture = "release/non-executable";
    });
  piePackage = harborLib.mkUpstreamBinaryPackage (args
    // {
      inherit pkgs;
      binaries.fixture = "release/static-pie";
    });
  directorySource =
    pkgs.runCommand "upstream-unpacked-source" {
      nativeBuildInputs = [pkgs.gnutar pkgs.gzip];
    } ''
      mkdir -p "$out"
      tar -xzf ${fixture}/fixture.tar.gz -C "$out"
    '';
  directoryPackage = harborLib.mkUpstreamBinaryPackage (args
    // {
      inherit pkgs;
      # A locked non-flake input exposes its unpacked tree through outPath.
      sources.${system} = {outPath = directorySource;};
    });
  rejects = extra: !(builtins.tryEval (harborLib.mkUpstreamBinaryPackage (args // {inherit pkgs;} // extra)).drvPath).success;
  failureCases = {
    missing = "release/missing";
    directory = "release";
    symlink = "release/link";
    parent-symlink = "release/linked/fixture";
    non-executable = "release/non-executable";
    script = "release/script";
    wrong-machine = "release/wrong-machine";
    dynamic = "release/dynamic";
    dynamic-needed = "release/dynamic-needed";
    relocatable = "release/relocatable";
    core = "release/core";
    shared-library = "release/shared-library";
    no-load = "release/no-load";
    non-executable-load = "release/non-executable-load";
    unmapped-entry = "release/unmapped-entry";
  };
  legacySource =
    pkgs.runCommand "legacy-binary-source" {
      nativeBuildInputs = [pkgs.gnutar pkgs.gzip];
    } ''
      tar -xzf ${fixture}/fixture.tar.gz
      mkdir -p "$out/bin"
      cp release/fixture "$out/bin/fixture"
      cat > "$out/manifest.json" <<'EOF'
      {"schemaVersion":1,"name":"upstream-fixture","version":"1","system":"${system}","binaries":["fixture"]}
      EOF
    '';
  legacyPackage = harborLib.mkReleaseBinaryPackage {
    inherit pkgs;
    inherit (args) pname version;
    sources.${system} = legacySource;
    binaries = ["fixture"];
  };
in {
  upstream-binary-target-selection =
    pkgs.runCommand "check-upstream-binary-target-selection" {
      nativeBuildInputs = [pkgs.gnutar pkgs.gzip pkgs.binutils];
    } (lib.concatMapStringsSep "\n" (targetSystem: let
        selected = harborLib.mkUpstreamBinaryPackage {
          inherit pkgs targetSystem;
          inherit (args) pname version;
          sources = lib.genAttrs ["x86_64-linux" "aarch64-linux"] (target: "${fixture}/${target}.tar.gz");
          binaries.fixture = "${targetSystem}/fixture";
        };
      in ''
        tar -xzf ${fixture}/${targetSystem}.tar.gz
        cmp ${targetSystem}/fixture ${selected}/bin/fixture
        readelf -h ${selected}/bin/fixture | grep -F ${lib.escapeShellArg (
          if targetSystem == "aarch64-linux"
          then "AArch64"
          else "Advanced Micro Devices X86-64"
        )}
      '') ["x86_64-linux" "aarch64-linux"]
      + ''
        touch "$out"
      '');
  upstream-binary-legacy-contract = pkgs.runCommand "check-upstream-binary-legacy-contract" {} ''
    cmp ${legacySource}/bin/fixture ${legacyPackage}/bin/fixture
    ${legacyPackage}/bin/fixture
    mkdir invalid
    sed 's/"version":"1"/"version":"2"/' ${legacySource}/manifest.json > invalid/manifest.json
    output="$out"
    export out="$PWD/rejected"
    phase=${lib.escapeShellArg legacyPackage.installPhase}
    legacy=${lib.escapeShellArg (toString legacySource)}
    phase="''${phase//$legacy/$PWD/invalid}"
    if bash -euo pipefail -c "$phase" > log 2>&1; then
      echo 'accepted mismatched legacy manifest' >&2
      exit 1
    fi
    grep -F 'manifest does not match' log
    touch "$output"
  '';
  upstream-binary-installation =
    pkgs.runCommand "check-upstream-binary-installation" {
      nativeBuildInputs = [pkgs.gnutar pkgs.gzip];
    } ''
      tar -xzf ${fixture}/fixture.tar.gz
      cmp release/fixture ${package}/bin/fixture
      cmp release/fixture ${xzPackage}/bin/fixture
      cmp release/non-executable ${modePackage}/bin/fixture
      cmp release/static-pie ${piePackage}/bin/fixture
      ${piePackage}/bin/fixture
      cmp release/fixture ${directoryPackage}/bin/fixture
      ${directoryPackage}/bin/fixture
      test -x ${modePackage}/bin/fixture
      ${modePackage}/bin/fixture
      test -x ${package}/bin/fixture
      ${package}/bin/fixture
      test "$(find ${package} -type f | wc -l)" -eq 1
      touch "$out"
    '';
  upstream-binary-invalid-mappings = assert rejects {sources = {};};
  assert rejects {targetSystem = "x86_64-darwin";};
  assert rejects {requireExecutable = "false";};
  assert rejects {binaries = {};};
  assert rejects {binaries."../escape" = "release/fixture";};
  assert rejects {binaries.fixture = "/release/fixture";};
  assert rejects {binaries.fixture = "release/../fixture";};
  assert rejects {binaries.fixture = "release//fixture";};
  assert rejects {binaries.fixture = "release/fixture\n";};
    pkgs.runCommand "check-upstream-binary-invalid-mappings" {} "touch $out";
  upstream-binary-invalid-payloads =
    pkgs.runCommand "check-upstream-binary-invalid-payloads" {
      nativeBuildInputs = [pkgs.gnutar pkgs.gzip];
    } (lib.concatStringsSep "\n" (lib.mapAttrsToList (name: path: let
          rejected = harborLib.mkUpstreamBinaryPackage (args
            // {
              inherit pkgs;
              binaries.fixture = path;
            });
        in ''
          mkdir ${lib.escapeShellArg name}
          (
            cd ${lib.escapeShellArg name}
            tar -xzf ${fixture}/fixture.tar.gz
            export out="$PWD/installed"
            runHook() { :; }
            if bash -euo pipefail -c ${lib.escapeShellArg ("runHook() { :; }; " + rejected.installPhase)} > log 2>&1; then
              echo "accepted invalid upstream payload: ${name}" >&2
              exit 1
            fi
            grep -F 'upstream binary' log
          )
        '')
        failureCases)
      + ''
        touch "$out"
      '');
}
