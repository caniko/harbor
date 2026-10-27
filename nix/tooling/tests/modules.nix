{
  self,
  inputs,
  pkgs,
}: let
  inherit (pkgs) lib;
  system = pkgs.stdenv.hostPlatform.system;
  home = modules:
    inputs.home-manager.lib.homeManagerConfiguration {
      inherit pkgs;
      modules =
        [
          self.homeManagerModules.default
          {
            home = {
              username = "fixture";
              homeDirectory = "/home/fixture";
              stateVersion = "26.05";
            };
          }
        ]
        ++ modules;
    };
  disabled = home [{programs.harbor.frameworks = ["cad" "android" "ntt"];}];
  python = home [
    {
      programs.harbor = {
        enable = true;
        frameworks = ["python"];
      };
    }
  ];
  override = pkgs.writeShellScriptBin "uv" ''printf '%s\n' harbor-uv-override'';
  overridden = home [
    {
      programs.harbor = {
        enable = true;
        frameworks = ["python"];
        tools.uv.package = override;
        tools.pyright.enable = false;
      };
    }
  ];
  javascript = home [
    {
      programs.harbor = {
        enable = true;
        frameworks = ["javascript" "projects"];
      };
    }
  ];
  cadPackage = self.packages.${system}.cad-default.overrideAttrs (_: {pname = "harbor-cad-override";});
  cad = home [
    {
      programs.harbor = {
        enable = true;
        frameworks = ["cad"];
        tools.cad.package = cadPackage;
        tools.blender.mcp.autoConnect = false;
      };
      programs.harbor-cad.worker = {
        enable = true;
        profile = self + "/components/cad/profiles/ci.json";
      };
    }
  ];
  masterOff = home [{programs.harbor.tools.blender.enable = true;}];
  assertGood = c: lib.all (a: a.assertion) c.config.assertions;
  rejected = c: !(builtins.tryEval c.activationPackage.drvPath).success;
  presetNames = lib.filter (framework:
    (self.lib.tooling.resolve {
      inherit system;
      frameworks = [framework];
    }).unsupported
    == [])
  (builtins.attrNames self.lib.tooling.catalog.frameworks);
  preset = framework: {
    programs.harbor = {
      enable = true;
      frameworks = [framework];
      settings.android = {
        platformVersions = ["34"];
        buildToolsVersions = ["34.0.0"];
        ndkVersions = ["29.0.14206865"];
      };
      # NTT pins are consumer-owned. Its independently configured package
      # override is the supported installer path exercised by this fixture.
      tools.ntt.package = pkgs.hello;
    };
  };
  presets = lib.genAttrs presetNames (framework: home [(preset framework)]);
  nixos = modules:
    inputs.nixpkgs.lib.nixosSystem {
      inherit system;
      modules =
        [
          self.nixosModules.default
          {
            boot.isContainer = true;
            system.stateVersion = "26.05";
          }
        ]
        ++ modules;
    };
  systemPresets = lib.genAttrs presetNames (framework: nixos [(preset framework)]);
  systemCad = nixos [
    {
      programs.harbor = {
        enable = true;
        frameworks = ["cad"];
      };
      services.harbor-cad.package = cadPackage;
    }
  ];
  missingAndroid = home [
    {
      programs.harbor = {
        enable = true;
        frameworks = ["android"];
      };
    }
  ];
  missingNtt = home [
    {
      programs.harbor = {
        enable = true;
        frameworks = ["ntt"];
      };
    }
  ];
  sdkDisabled = home [
    {
      programs.harbor = {
        enable = true;
        frameworks = ["android"];
        tools.android.enable = false;
      };
    }
  ];
  unsupported = home [
    {
      programs.harbor = {
        enable = true;
        tools.llm.enable = true;
      };
    }
  ];
  conflict = home [
    {
      programs.harbor = {
        enable = true;
        frameworks = ["ntt" "evm"];
        tools.ntt.package = pkgs.hello;
      };
    }
  ];
  dependencyDisabled = home [
    {
      programs.harbor = {
        enable = true;
        tools.blender.enable = false;
        tools.blender.mcp.enable = true;
      };
    }
  ];
  pencilOverride = self.packages.${system}.open-pencil-cli.overrideAttrs (_: {pname = "open-pencil-override";});
  pencil = home [
    {
      programs.harbor = {
        enable = true;
        frameworks = ["javascript"];
        tools.openpencil.package = pencilOverride;
      };
    }
  ];
  mismatchedPencil = home [
    {
      programs.harbor = {
        enable = true;
        frameworks = ["javascript"];
        tools.openpencil.package = pencilOverride.overrideAttrs (_: {
          version = "0.14.0";
          __intentionallyOverridingVersion = true;
        });
      };
    }
  ];
  missingFleetix = home [self.homeManagerModules.fleetix];
  fleetix = home [
    self.homeManagerModules.fleetix
    (import (inputs.fleetix + "/modules/mcp.nix") {fleetixLib = import (inputs.fleetix + "/lib") {inherit lib;};})
    {
      programs.harbor = {
        enable = true;
        frameworks = ["javascript"];
      };
      programs.opencode.enable = true;
      fleetix.mcp = {
        enable = true;
        package = pkgs.hello;
        harnesses.opencode.enable = true;
      };
    }
  ];
  missingSkillnet = home [self.homeManagerModules.skillnet];
  skillnet = home [
    self.homeManagerModules.skillnet
    {options.programs.skillnet.externalManifests = lib.mkOption {type = lib.types.listOf lib.types.str;};}
    {
      programs.harbor = {
        enable = true;
        skills = ["dataset-report"];
      };
      programs.skillnet.externalManifests = ["first"];
    }
  ];
  integrated = inputs.nixpkgs.lib.nixosSystem {
    inherit system;
    modules = [
      self.nixosModules.default
      inputs.home-manager.nixosModules.home-manager
      {
        boot.isContainer = true;
        system.stateVersion = "26.05";
        users.users.fixture.isNormalUser = true;
        programs.harbor = {
          enable = true;
          frameworks = ["python"];
        };
        home-manager.useGlobalPkgs = true;
        home-manager.users.fixture = {
          imports = [self.homeManagerModules.default];
          home.stateVersion = "26.05";
          programs.harbor.inheritSystemSelection = true;
        };
        users.users.independent.isNormalUser = true;
        home-manager.users.independent = {
          imports = [self.homeManagerModules.default];
          home.stateVersion = "26.05";
        };
        users.users.overriding.isNormalUser = true;
        home-manager.users.overriding = {
          imports = [self.homeManagerModules.default];
          home.stateVersion = "26.05";
          programs.harbor = {
            inheritSystemSelection = true;
            frameworks = ["core"];
            tools.uv.enable = false;
          };
        };
      }
    ];
  };
  result =
    {
      disabled = disabled.activationPackage.drvPath;
      python = python.activationPackage.drvPath;
      overrides = overridden.activationPackage.drvPath;
      javascript = javascript.activationPackage.drvPath;
      presets = lib.mapAttrs (_: c: c.activationPackage.drvPath) presets;
      sdkDisabled = sdkDisabled.activationPackage.drvPath;
      fleetix = fleetix.activationPackage.drvPath;
      skillnet = skillnet.activationPackage.drvPath;
    }
    // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
      integrated = integrated.config.system.build.toplevel.drvPath;
      systemPresets = lib.mapAttrs (_: c: c.config.system.build.toplevel.drvPath) systemPresets;
    }
    // lib.optionalAttrs (system == "x86_64-linux") {
      cad = cad.activationPackage.drvPath;
      systemCad = systemCad.config.system.build.toplevel.drvPath;
    };
in
  assert import ./resolver.nix {inherit lib;};
  assert disabled.config.programs.harbor.resolved.selected == [];
  assert disabled.config.programs.direnv.nix-direnv.package == pkgs.nix-direnv;
  assert !pkgs.stdenv.hostPlatform.isLinux || python.config.programs.direnv.nix-direnv.package == self.packages.${system}.core-nix-direnv;
  assert masterOff.config.programs.harbor.resolved.selected == [];
  assert !(masterOff.config.xdg.dataFile ? "blender-mcp-extension");
  assert assertGood python && assertGood overridden && assertGood javascript;
  assert lib.all assertGood (builtins.attrValues presets);
  assert assertGood sdkDisabled;
  assert !(builtins.tryEval missingAndroid.config.programs.harbor.tools.android.package.drvPath).success;
  assert system != "x86_64-linux" || !(builtins.tryEval missingNtt.config.programs.harbor.tools.ntt.package.drvPath).success;
  assert pkgs.stdenv.hostPlatform.isLinux || rejected unsupported;
  assert system != "x86_64-linux" || rejected conflict;
  assert rejected dependencyDisabled;
  assert rejected missingFleetix;
  assert rejected missingSkillnet;
  assert assertGood skillnet && skillnet.config.programs.skillnet.externalManifests == ["first" "${self.packages.${system}.llm-skills}/Skillnet.pkl"];
  assert assertGood fleetix;
  assert !pkgs.stdenv.hostPlatform.isLinux
  || (
    integrated.config.home-manager.users.fixture.programs.harbor.frameworks
    == ["python"]
    && !integrated.config.home-manager.users.independent.programs.harbor.enable
    && integrated.config.home-manager.users.overriding.programs.harbor.frameworks == ["core"]
    && !(lib.elem "uv" integrated.config.home-manager.users.overriding.programs.harbor.resolved.selected)
  );
  assert lib.elem override overridden.config.home.packages;
  assert !(lib.elem pkgs.uv overridden.config.home.packages);
  assert !(lib.elem "pyright" overridden.config.programs.harbor.resolved.selected);
  assert builtins.length (lib.filter (p: (p.pname or "") == "open-pencil-cli") javascript.config.home.packages) == 1;
  assert system
  != "x86_64-linux"
  || (
    assertGood cad
    && lib.elem cadPackage cad.config.home.packages
    && lib.any (lib.hasPrefix "${cadPackage}/bin/harbor-cad worker") cad.config.systemd.user.services.harbor-cad.Service.ExecStart
    && cad.config.programs.harbor.mcp.servers.blender.enabled == false
    && cad.config.programs.harbor.mcp.servers.harbor-cad.env.HARBOR_CAD_SOCKET == "/home/fixture/.local/state/harbor-cad/worker.sock"
    && javascript.config.programs.harbor.mcp.servers.open-pencil.command == "${javascript.config.programs.harbor.tools.openpencil.package}/bin/openpencil-mcp"
    && systemCad.config.programs.harbor.tools.cad.package == cadPackage
    && lib.elem cadPackage systemCad.config.environment.systemPackages
    && systemCad.config.environment.etc."harbor/cad/runtime-cpu.json".source == self.packages.${system}.cad-runtime-cpu
    && pencil.config.programs.harbor.tools.openpencil.desktop.package.cli == pencilOverride
    && fleetix.config.programs.opencode.settings.mcp.servers.open-pencil.command == [fleetix.config.programs.harbor.mcp.servers.open-pencil.command]
  );
  assert system != "x86_64-linux" || rejected mismatchedPencil;
    pkgs.runCommand "harbor-module-contracts" {
      receipts = builtins.unsafeDiscardStringContext (builtins.toJSON result);
      passAsFile = ["receipts"];
    } ''
      mkdir -p "$out"
      cp "$receiptsPath" "$out/evaluation.json"
      # Build real Home Manager package environments and exercise both the
      # ordinary binary and the authoritative override on their resulting PATHs.
      ${python.activationPackage}/home-path/bin/uv --version
      test "$(${overridden.activationPackage}/home-path/bin/uv --version)" = harbor-uv-override
    ''
