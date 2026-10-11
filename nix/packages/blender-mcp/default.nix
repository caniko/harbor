{pkgs}: let
  version = "1.18.0";
  rev = "37acac7fd25d424b23c8a84f27d6c16c848d810d";
  src = pkgs.fetchzip {
    url = "https://github.com/newo-ether/blender-mcp/archive/${rev}.tar.gz";
    hash = "sha256-foEPi2TxLM1ySzssIEzceP7y0PAFHJCBJznYwTmKMQw=";
  };

  blenderMcp = pkgs.python3Packages.buildPythonApplication {
    pname = "blender-mcp";
    inherit version src;
    pyproject = true;

    build-system = with pkgs.python3Packages; [
      setuptools
      wheel
    ];

    # Upstream requires mcp[cli]>=1.3.0,<2 plus httpx; tomli is only needed
    # below Python 3.11. The cli extra is python-dotenv + typer.
    dependencies = with pkgs.python3Packages; [
      mcp
      python-dotenv
      typer
      httpx
    ];

    patches = [
      ./disable-telemetry.patch
      ./registry-only.patch
      ./discovery-timeouts.patch
      ./serialization-vectors.patch
      ./screenshot-guard.patch
      ./software-viewport-capture.patch
    ];

    # Keep upstream coverage while adapting the two deliberate local policy
    # changes: registry-only targeting, and a bounded handshake on discovery.
    postPatch = ''
      cp ${./test_integration.py} tests/test_canix_integration.py
      substituteInPlace tests/test_host_connection.py \
        --replace-fail 'test_fallback_uses_named_loopback_defaults' 'test_no_registry_rejects_default_endpoint' \
        --replace-fail 'test_explicit_endpoint_override_is_preserved' 'test_endpoint_override_cannot_bypass_registry' \
        --replace-fail $'            connection = host.get_blender_connection()\n        self.assertEqual(connection.host, DEFAULT_BRIDGE_HOST)\n        self.assertEqual(connection.port, DEFAULT_BRIDGE_PORT)\n        self.assertTrue(connection.connected)' $'            with self.assertRaises(BlenderMCPError) as rejected:\n                host.get_blender_connection()\n            self.assertEqual(rejected.exception.code, "no_registered_instances")\n            self.assertEqual(_FakeConnection.created, [])\n        self.assertIsNone(host.blender_connection)' \
        --replace-fail $'            connection = host.get_blender_connection()\n        self.assertEqual((connection.host, connection.port), ("localhost", 19876))' $'            with self.assertRaises(BlenderMCPError) as rejected:\n                host.get_blender_connection()\n            self.assertEqual(rejected.exception.code, "no_registered_instances")\n            self.assertEqual(_FakeConnection.created, [])\n        self.assertIsNone(host.blender_connection)'
      substituteInPlace tests/test_instance_registry.py \
        --replace-fail 'def send_command(self, command, params=None):' 'def send_command(self, command, params=None, *, timeout=None):' \
        --replace-fail '        if command == "blender_mcp_handshake":' $'        if command == "blender_mcp_handshake":\n            assert timeout is None or 0 < timeout <= 10, timeout'
    '';
    nativeCheckInputs = [pkgs.python3Packages.pytestCheckHook];
    # This one suite makes real documentation HTTP requests. All other
    # upstream Python suites remain enabled in the Nix network sandbox.
    disabledTestPaths = ["tests/test_blender_docs_live.py"];
    DISABLE_TELEMETRY = "true";

    pythonImportsCheck = [
      "blender_mcp"
      "blender_mcp.app"
      "blender_mcp.host"
    ];

    meta = with pkgs.lib; {
      description = "Blender integration through the Model Context Protocol (pinned v1.18.0)";
      homepage = "https://github.com/newo-ether/blender-mcp";
      license = licenses.mit;
      mainProgram = "blender-mcp";
      maintainers = [];
    };
  };

  # Matching Blender extension payload. Layout mirrors the upstream
  # extension builder staging: extension modules at the root plus schemas/
  # and LICENSE beside blender_manifest.toml. Runtime discovery uses this
  # immutable tree; the deterministic ZIP is a standalone distribution artifact.
  blenderMcpExtension = pkgs.stdenvNoCC.mkDerivation {
    pname = "blender-mcp-extension";
    inherit version src;
    # Extension-side compatibility patches (same pinned source; the app
    # patches above do not apply here).
    patches = [
      ./serialization-vectors.patch
      ./screenshot-guard.patch
      ./software-viewport-capture.patch
    ];
    dontConfigure = true;
    dontBuild = true;
    installPhase = ''
      runHook preInstall
      dest="$out/share/blender-mcp-extension/blender_mcp"
      mkdir -p "$dest/schemas"
      cp blender_extension/__init__.py "$dest/"
      for f in blender_extension/*.py; do
        cp "$f" "$dest/"
      done
      for d in blender_extension/*/; do
        cp -r "$d" "$dest/"
      done
      cp blender_extension/blender_manifest.toml "$dest/"
      cp LICENSE "$dest/"
      cp schemas/*.json "$dest/schemas/"
      ${pkgs.python3}/bin/python3 - "$dest" "$out/share/blender-mcp-extension/blender_mcp-${version}.zip" <<'PY'
      import os
      import sys
      import zipfile

      root, output = sys.argv[1], sys.argv[2]
      entries = []
      for dirpath, _dirnames, filenames in os.walk(root):
          for name in filenames:
              entries.append(os.path.join(dirpath, name))
      with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
          for path in sorted(entries):
              info = zipfile.ZipInfo(
                  os.path.relpath(path, root), date_time=(1980, 1, 1, 0, 0, 0)
              )
              info.compress_type = zipfile.ZIP_DEFLATED
              with open(path, "rb") as handle:
                  archive.writestr(info, handle.read())
      PY
      runHook postInstall
    '';
    meta = with pkgs.lib; {
      description = "Blender MCP extension payload matching blender-mcp v1.18.0";
      homepage = "https://github.com/newo-ether/blender-mcp";
      license = licenses.mit;
      maintainers = [];
    };
  };

  # Canonical agent skill with all relative references preserved. The
  # skill-policy patch scopes upstream layout mandates to requested
  # layout work and makes motion-blur changes conditional; lifecycle,
  # transaction, and verification guidance is retained verbatim.
  blenderMcpSkill = pkgs.stdenvNoCC.mkDerivation {
    pname = "blender-mcp-skill";
    inherit version src;
    patches = [./skill-policy.patch];
    dontConfigure = true;
    dontBuild = true;
    installPhase = ''
      runHook preInstall
      mkdir -p "$out/share/skills"
      cp -r skills/blender-mcp "$out/share/skills/blender-mcp"
      runHook postInstall
    '';
    meta = with pkgs.lib; {
      description = "Blender MCP agent skill (pinned v1.18.0)";
      homepage = "https://github.com/newo-ether/blender-mcp";
      license = licenses.mit;
      maintainers = [];
    };
  };
in {
  blender-mcp-test-env = pkgs.python3.withPackages (ps: [ps.mcp ps.pillow]);
  blender-mcp = blenderMcp;
  blender-mcp-extension = blenderMcpExtension;
  blender-mcp-skill = blenderMcpSkill;
}
