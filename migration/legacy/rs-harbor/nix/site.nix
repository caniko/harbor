{
  pkgs,
  lib,
  projectSiteLib,
}: let
  docs = pkgs.stdenv.mkDerivation {
    pname = "rs-harbor-docs";
    version = "0.1.0";
    src = ../docs;
    nativeBuildInputs = [pkgs.mdbook];
    phases = ["buildPhase" "installPhase"];
    buildPhase = ''
      cp -r --no-preserve=mode $src docs
      chmod -R u+w docs
      mdbook build docs
    '';
    installPhase = ''
      mkdir -p $out
      cp -r docs/book/. $out/
    '';
  };

  website = projectSiteLib.mkProjectSite {
    pname = "rs-harbor-website";
    domain = "rs-harbor.tartanoglu.com";
    configPath = ../website/plinth-project.toml;
    docsPackage = docs;
    staticPaths = [
      {
        source = ../website/static/rs-harbor-mark.svg;
        target = "website/static/rs-harbor-mark.svg";
      }
    ];
  };
in {
  inherit website docs;

  site = website;
}
