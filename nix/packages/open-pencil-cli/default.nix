# Hermetic @open-pencil/cli (https://github.com/open-pencil/open-pencil, NOT the ZSeven-W Rust app).
#
# The published tarball ships prebuilt dist/ with no install scripts, so the
# closure is reconstructed by unpacking the registry tarballs listed in
# deps.nix (generated from the committed package-lock.json) instead of
# running npm at build time. No network access during build.
#
# Mirrors the SynDB derivation (nix/open-pencil-cli); regenerate both from
# the same package-lock.json. Extraction to a shared flake is follow-up.
{
  lib,
  stdenvNoCC,
  fetchurl,
  nodejs,
}: let
  version = "0.15.1";
  deps = import ./deps.nix;
  mainTarball = fetchurl {
    url = "https://registry.npmjs.org/@open-pencil/cli/-/cli-0.15.1.tgz";
    hash = "sha512-GyVlrHKC4rHCbFfhn54nqg2Mr884kYlcdcXM/dvnrhBIQBPvoz/RYDzecAdmhFZj49nbnxXp2/LHi9F6C5Q8Vg==";
  };
  depTarballs = map (d:
    fetchurl {
      inherit (d) url hash;
      name = d.file;
    })
  deps;
  unpackList = builtins.toFile "unpack-list" (
    lib.concatMapStrings (d: "${d.file} ${d.dest}\n") deps
  );
in
  stdenvNoCC.mkDerivation {
    pname = "open-pencil-cli";
    inherit version;

    src = mainTarball;
    srcs = depTarballs;

    nativeBuildInputs = [nodejs];

    dontUnpack = true;
    dontConfigure = true;
    dontBuild = true;

    installPhase = ''
      runHook preInstall
      mkdir -p $out/lib/open-pencil-cli
      tar -xzf $src -C $out/lib/open-pencil-cli --strip-components=1
      mkdir -p $out/lib/open-pencil-cli/node_modules
      while read -r file dest; do
        target=$out/lib/open-pencil-cli/node_modules/$dest
        # Store paths are <hash>-<file>; strip the hash before matching.
        match=""
        for candidate in $srcs; do
          base=$(basename "$candidate")
          if [ "''${base#*-}" = "$file" ]; then
            match=$candidate
            break
          fi
        done
        if [ -z "$match" ]; then
          echo "missing tarball for $dest ($file)" >&2
          exit 1
        fi
        mkdir -p "$target"
        tar -xzf "$match" -C "$target" --strip-components=1
      done < ${unpackList}
      mkdir -p $out/bin
      ln -s $out/lib/open-pencil-cli/bin/openpencil.js $out/bin/openpencil
      # The MCP servers ship prebuilt inside the CLI closure (@open-pencil/mcp).
      ln -s $out/lib/open-pencil-cli/node_modules/@open-pencil/mcp/dist/stdio.mjs $out/bin/openpencil-mcp
      ln -s $out/lib/open-pencil-cli/node_modules/@open-pencil/mcp/dist/index.mjs $out/bin/openpencil-mcp-http
      patchShebangs $out/lib/open-pencil-cli/bin $out/lib/open-pencil-cli/node_modules $out/bin
      runHook postInstall
    '';

    meta = {
      description = "OpenPencil CLI + MCP servers (open-pencil/open-pencil): inspect, export, and lint .fig design documents";
      mainProgram = "openpencil";
    };
  }
