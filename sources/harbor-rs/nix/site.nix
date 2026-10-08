{
  pkgs,
  lib,
  projectSiteLib,
  harborDocs,
}: let
  docs = harborDocs.mkDocs {
    inherit pkgs;
    src = ../docs;
    pname = "harbor-rs-docs";
  };

  website = harborDocs.mkSite {
    inherit projectSiteLib docs;
    pname = "harbor-rs-website";
    domain = "harbor-rs.tartanoglu.com";
    configPath = ../website/plinth-project.toml;
    staticPaths = [
      {
        source = ../website/static/harbor-rs-mark.svg;
        target = "website/static/harbor-rs-mark.svg";
      }
    ];
  };
in {
  inherit website docs;

  site = website;
}
