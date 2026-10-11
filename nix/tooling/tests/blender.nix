{
  self,
  inputs,
  pkgs,
}: let
  home = inputs.home-manager.lib.homeManagerConfiguration {
    inherit pkgs;
    modules = [
      self.homeManagerModules.default
      {
        home = {
          username = "fixture";
          homeDirectory = "/build/harbor-mcp-home";
          stateVersion = "26.05";
        };
        programs.harbor = {
          enable = true;
          tools.blender = {
            enable = true;
            mcp.autoConnect = false;
            mcp.workspace = "/build/harbor-blender-proof/workspace";
          };
        };
      }
    ];
  };
  bridge = home.config.programs.harbor.mcp.servers.blender.command;
  extension = home.config.programs.harbor.tools.blender.mcp.extensionPackage;
  python = self.packages.${pkgs.stdenv.hostPlatform.system}.blender-mcp-test-env;
  desktop = pkgs.lib.findFirst (p: p.name == "blender-mcp-desktop") (throw "missing Blender desktop launcher") home.config.home.packages;
  candidate = pkgs.linkFarm "harbor-blender-acceptance" [
    {
      name = "server";
      path = bridge;
    }
    {
      name = "desktop";
      path = "${desktop}/bin/blender-mcp-desktop";
    }
    {
      name = "blender";
      path = "${home.config.programs.harbor.tools.blender.package}/bin/blender";
    }
    {
      name = "python";
      path = "${python}/bin/python";
    }
    {
      name = "root";
      path = pkgs.writeText "harbor-blender-proof-root" "/build/harbor-blender-proof";
    }
  ];
in
  pkgs.runCommand "harbor-blender-mcp-runtime" {
    nativeBuildInputs = [python pkgs.xvfb-run];
  } ''
      ${pkgs.python3}/bin/python3 -B ${../../packages/blender-mcp/test_startup.py} ${../../modules/blender-mcp-startup.py}
    # The sandbox has no host /run/opengl-driver. Both Xvfb and Blender must
    # resolve the same store-backed Mesa GLX vendor and software DRI driver.
    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath [pkgs.mesa pkgs.libglvnd]}"
    export LIBGL_DRIVERS_PATH="${pkgs.mesa}/lib/dri"
    export __GLX_VENDOR_LIBRARY_NAME=mesa
    export __EGL_VENDOR_LIBRARY_DIRS="${pkgs.mesa}/share/glvnd/egl_vendor.d"
    export LIBGL_ALWAYS_SOFTWARE=1
    export OMP_NUM_THREADS=2
    if ! xvfb-run -a -s '-screen 0 1280x720x24 -nolisten tcp' \
      python3 -B ${./blender-acceptance.py} --candidate ${candidate} \
      --state-root /build/harbor-blender-proof/state --interactive --save-test; then
      tail -n 5 /build/harbor-blender-proof/state/mcp.log
      cat /build/harbor-blender-proof/state/blender.log
      exit 1
    fi
      test -f ${extension}/share/blender-mcp-extension/blender_mcp/blender_manifest.toml
      test -f ${extension}/share/blender-mcp-extension/blender_mcp/schemas/node-tree-patch-v1.json
      mkdir -p "$out"
      cp /build/harbor-blender-proof/state/result.json /build/harbor-blender-proof/state/viewport.png "$out/"
  ''
