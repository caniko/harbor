# Retained portable-release and archive-timezone regressions from harbor-rs.
{
  pkgs,
  self,
  system,
}: {
  portable-release-extra-files = let
    fixturePackage = pkgs.writeShellScriptBin "player" "echo fixture";
    license = pkgs.writeText "fixture-license" "MIT fixture\n";
    release = self.lib.mkPortableBinaryRelease {
      inherit pkgs;
      pname = "player";
      version = "0.2.0";
      artifacts.${system} = {
        bundler = _: pkgs.writeScript "fixture-bundle" "#!/bin/sh\necho fixture\n";
        entries.player.package = fixturePackage;
        extraFiles."LICENSE" = {
          source = license;
          mode = "0644";
        };
        extraFiles."docs/README.txt" = {source = license;};
        extraFiles."start-player" = {
          source = license;
          mode = "0755";
        };
        extraFiles."private/readme" = {
          source = license;
          mode = "0600";
        };
      };
    };
  in
    pkgs.runCommand "check-portable-release-extra-files" {
      nativeBuildInputs = [pkgs.coreutils pkgs.gnutar pkgs.gzip];
    } ''
      tar -xzf ${release.releaseBundle}/player-0.2.0-${system}-nix-bundle.tar.gz
      test "$(stat -c %a LICENSE)" = 644
      test "$(stat -c %a docs/README.txt)" = 644
      test "$(stat -c %a start-player)" = 755
      test "$(stat -c %a private/readme)" = 600
      test "$(stat -c %a bin/player)" = 755
      cmp LICENSE ${license}
      test "$(stat -c %Y LICENSE)" = 0
      touch "$out"
    '';

  release-file-destination-validation = let
    common = import ../lib/release-common.nix {inherit (pkgs) lib;};
    attempt = files:
      builtins.tryEval (builtins.deepSeq (common.stageFiles {
          inherit files;
          root = "$out";
          reserved = ["manifest.json" "bin/player"];
        })
        true);
    file = {source = ../LICENSE-MIT;};
    rejected = [
      {"../LICENSE" = file;}
      {"/LICENSE" = file;}
      {"a//b" = file;}
      {"a/./b" = file;}
      {"manifest.json" = file;}
      {"bin" = file;}
      {"bin/player/child" = file;}
      {
        "a" = file;
        "a/b" = file;
      }
      {"LICENSE" = file // {mode = "8888";};}
      {"LICENSE" = {};}
    ];
  in
    assert builtins.all (files: !(attempt files).success) rejected;
    assert (attempt {"docs/License with spaces" = file;}).success;
      pkgs.runCommand "check-release-file-destination-validation" {} "touch $out";

  release-archive-modes-and-reproducibility = let
    source = pkgs.writeText "archive-mode-fixture" "supporting file\n";
    mkArchive = format: archiveTimezone:
      self.lib.mkReleaseArchive {
        inherit pkgs format archiveTimezone;
        pname = "fixture";
        version = "0.1.0";
        name = "fixture.${format}";
        package = source;
        entries = {
          "License with spaces" = {inherit source;};
          "bin/start" = {
            inherit source;
            mode = "0755";
          };
        };
      };
    tarArchive = mkArchive "tar.gz" "UTC";
    zipArchive = mkArchive "zip" "UTC";
    namedZoneZip = mkArchive "zip" "Europe/Istanbul";
  in
    pkgs.runCommand "check-release-archive-modes-and-reproducibility" {
      nativeBuildInputs = [pkgs.coreutils pkgs.gnutar pkgs.gzip pkgs.unzip pkgs.zip];
    } ''
      export TZ=UTC
      mkdir tar-stage zip-stage
      tar -xzf ${tarArchive}/fixture.tar.gz -C tar-stage
      unzip -q ${zipArchive}/fixture.zip -d zip-stage
      test "$(stat -c %a 'tar-stage/License with spaces')" = 644
      test "$(stat -c %a 'zip-stage/License with spaces')" = 644
      test -x tar-stage/bin/start
      test -x zip-stage/bin/start
      tar --sort=name --owner=0 --group=0 --numeric-owner --mtime='@0' \
        -czf rebuilt.tar.gz -C tar-stage .
      cmp rebuilt.tar.gz ${tarArchive}/fixture.tar.gz
      cd zip-stage
      printf '%s\n' './License with spaces' './bin/start' | zip -X -q ../rebuilt.zip -@
      cmp ../rebuilt.zip ${zipArchive}/fixture.zip
      cmp ${zipArchive}/fixture.zip ${namedZoneZip}/fixture.zip
      touch "$out"
    '';
}
