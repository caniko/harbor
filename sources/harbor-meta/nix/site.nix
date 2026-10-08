{
  pkgs,
  lib,
  projectSiteLib,
}: let
  website = projectSiteLib.mkProjectSite {
    pname = "harbor-website";
    domain = "harbor.tartanoglu.com";
    configPath = ../website/plinth-project.toml;
    staticPaths = [
      {
        source = ../website/static/harbor-mark.svg;
        target = "website/static/harbor-mark.svg";
      }
    ];
  };
in {
  inherit website;
  site = website;
}
