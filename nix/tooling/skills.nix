{
  self,
  pkgs,
  # Flake outputs have no module config; only the providers a skill realizes are forced.
  config ? {programs.harbor.settings.tex.profile = "article";},
}: names: let
  inherit (pkgs) lib;
  tooling = self.lib.tooling;
  inherit (tooling) catalog;
  system = pkgs.stdenv.hostPlatform.system;
  providers = import ./packages.nix {inherit self pkgs config;};
  provider = name: tool:
    if tool == "texlive"
    then
      self.lib.tex.mkTexlive {
        inherit pkgs;
        profile = "article";
        extraPackages = ps: map (package: ps.${package}) (catalog.skills.${name}.requires.tex.extraPackages or []);
      }
    else providers.${tool};
  env = name:
    pkgs.buildEnv {
      name = "harbor-skill-${name}-env";
      paths = map (provider name) (tooling.skillTools system name);
      pathsToLink = ["/bin"];
    };
  source = name: let
    skill = catalog.skills.${name};
  in
    ../../components + "/${skill.owner}/${skill.source}";
  manifest = pkgs.writeText "Skillnet.pkl" ''
    class Skill {
      role: String = "entrypoint"
      dependencies: Listing<String> = new {}
      source: String? = null
      users: Listing<String>? = null
    }

    schemaVersion = 2
    skills: Mapping<String, Skill> = new {
    ${lib.concatMapStringsSep "\n" (name: "  [\"${name}\"] = new { source = \"skills/${name}\" }") names}
    }
  '';
in
  assert lib.assertMsg (lib.all (name: builtins.hasAttr name catalog.skills) names) "harbor: unknown skill selection";
    pkgs.runCommandLocal "harbor-skills" {} ''
      mkdir -p "$out/skills"
      cp ${manifest} "$out/Skillnet.pkl"
      ${lib.concatMapStringsSep "\n" (name: ''
          cp -R ${source name} "$out/skills/${name}"
          chmod -R u+w "$out/skills/${name}"
          ln -s ${env name} "$out/skills/${name}/env"
        '')
        names}
    ''
