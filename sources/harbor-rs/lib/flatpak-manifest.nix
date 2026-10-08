# mkFlatpakManifest :: { pkgs, appId, pname, desktopFile, icon?, runtime?, ... }
#                   -> { manifestText; manifestPath; }
#
# Generate a Flatpak manifest (JSON) for packaging a pre-built binary.
# The manifest is meant for use with flatpak-builder outside of Nix.
#
# Workflow:
#   1. nix build .#my-app            # build the binary
#   2. nix build .#flatpak-manifest  # generate the manifest
#   3. cp result/bin/my-app .        # copy binary next to manifest
#   4. flatpak-builder --user --install build-dir ./result
#
# Example:
#   flatpakManifest = harbor-rs.lib.mkFlatpakManifest {
#     inherit pkgs;
#     appId = "com.example.MyApp";
#     pname = "my-app";
#     desktopFile = "[Desktop Entry]\nType=Application\nName=My App\nExec=my-app";
#   };
{packageTests}: {
  pkgs,
  appId,
  pname,
  desktopFile,
  version ? "0.1.0",
  icon ? null,
  runtime ? "org.freedesktop.Platform",
  runtimeVersion ? "24.08",
  sdk ? "org.freedesktop.Sdk",
  sdkExtensions ? [],
  finishArgs ? [
    "--share=ipc"
    "--socket=x11"
    "--socket=wayland"
    "--device=dri"
    "--socket=pulseaudio"
  ],
  extraModules ? [],
}: let
  inherit (pkgs) lib;

  # Validate appId is reverse-DNS format (at least two components)
  validAppId = builtins.match "[a-zA-Z][a-zA-Z0-9_]*(\\.[a-zA-Z][a-zA-Z0-9_]*)+" appId;
in
  assert validAppId
  != null
  || throw "mkFlatpakManifest: appId must be reverse-DNS format (e.g. com.example.MyApp), got: ${appId}";
  assert builtins.isString pname
  && pname != ""
  || throw "mkFlatpakManifest: pname must be a non-empty string";
  assert builtins.isString desktopFile
  || throw "mkFlatpakManifest: desktopFile must be a string"; let
    iconSources =
      if icon != null
      then [
        {
          type = "file";
          path = builtins.toString icon;
        }
      ]
      else [];

    iconCommands =
      if icon != null
      then [
        "install -Dm644 ${builtins.baseNameOf (builtins.toString icon)} /app/share/icons/hicolor/256x256/apps/${appId}.png"
      ]
      else [];

    manifest =
      {
        "app-id" = appId;
        inherit runtime sdk;
        "runtime-version" = runtimeVersion;
        command = pname;
        "finish-args" = finishArgs;
      }
      // lib.optionalAttrs (sdkExtensions != []) {
        "sdk-extensions" = sdkExtensions;
      }
      // {
        modules =
          [
            {
              name = pname;
              buildsystem = "simple";
              "build-commands" =
                [
                  "install -Dm755 ${pname} /app/bin/${pname}"
                  "install -Dm644 ${pname}.desktop /app/share/applications/${appId}.desktop"
                ]
                ++ iconCommands;
              sources =
                [
                  {
                    type = "file";
                    path = pname;
                  }
                  {
                    type = "file";
                    path = "${pname}.desktop";
                  }
                ]
                ++ iconSources;
            }
          ]
          ++ extraModules;
      };

    manifestText = builtins.toJSON manifest;
    manifestPath = pkgs.writeText "${pname}-flatpak-manifest.json" manifestText;
    artifactBuilder = packageTests.mkArtifactBuilder {
      kind = "flatpak-builder";
      packageName = pname;
      inherit version;
      output = toString manifestPath;
      buildCommand = "nix build .#${pname}-flatpak-manifest";
      metadata = {
        inherit appId runtime runtimeVersion sdk sdkExtensions finishArgs;
        helper = "mkFlatpakManifest";
        outputKind = "flatpak-manifest";
      };
    };
  in {
    inherit manifestText manifestPath artifactBuilder;
  }
