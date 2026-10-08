# Replace `my-project` / `caniko/my-project` throughout after init.
{
  pkgs,
  lib,
  projectSiteLib,
  harborProjects,
}: let
  docs = harborProjects.mkDocs {
    inherit pkgs;
    src = ../docs;
    pname = "my-project-docs";
  };

  website = harborProjects.mkSite {
    inherit pkgs projectSiteLib docs;
    pname = "my-project-website";
    domain = "my-project.tartanoglu.com";
    configPath = ../website/plinth-project.toml;
    staticPaths = [
      {
        source = ../website/static/my-project-mark.svg;
        target = "website/static/my-project-mark.svg";
      }
    ];
    # Funnel into an embedded app at /app: pass the built web root
    # (index.html at its top level) once the project has one, e.g.
    #   appSource = "${my-app-web}/share/my-app-web";
    # then re-add the Platform nav/footer links and hero CTA pointing
    # at "/app". The bundle must emit subpath-safe URLs (for Dioxus,
    # build it with Dioxus.toml `[web.app] base_path = "app"`).
  };
in {
  inherit docs website;
  site = website;
}
