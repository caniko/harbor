# Build from the pinned upstream source so the patches are embedded
# in the actual Tauri application, rather than only patching the CLI closure.
{
  lib,
  rustPlatform,
  callPackage,
  pkg-config,
  wrapGAppsHook3,
  makeDesktopItem,
  copyDesktopItems,
  webkitgtk_4_1,
  gtk3,
  libsoup_3,
  openssl,
  fontconfig,
  freetype,
  cli ? callPackage ../open-pencil-cli {},
}: let
  frontend = callPackage ./frontend.nix {};
in
  rustPlatform.buildRustPackage {
    pname = "open-pencil-desktop";
    version = "0.15.1-canix3";
    src = callPackage ./source.nix {};
    cargoRoot = "desktop";
    buildAndTestSubdir = "desktop";
    cargoHash = "sha256-oHcAMHFk5kk4+SBv2bkTExikzWwX+Dy2Cbc/rex7KdQ=";
    nativeBuildInputs = [pkg-config wrapGAppsHook3 copyDesktopItems];
    buildInputs = [webkitgtk_4_1 gtk3 libsoup_3 openssl fontconfig freetype];
    buildFeatures = ["tauri/custom-protocol"];
    preBuild = ''
      cp -r ${frontend}/dist .
      cp -r ${frontend}/icons desktop/
      cp -r ${frontend}/generated desktop/
    '';
    preFixup = ''
      gappsWrapperArgs+=(--prefix PATH : ${lib.makeBinPath [cli]})
    '';
    postInstall = ''
      ln -s $out/bin/OpenPencil $out/bin/openpencil-desktop
      mkdir -p $out/share/icons/hicolor/128x128/apps
      cp ${frontend}/icons/128x128.png $out/share/icons/hicolor/128x128/apps/OpenPencil.png
    '';
    desktopItems = [
      (makeDesktopItem {
        name = "OpenPencil";
        desktopName = "OpenPencil";
        exec = "openpencil-desktop %U";
        icon = "OpenPencil";
        categories = ["Graphics"];
        mimeTypes = ["application/x-figma" "application/x-pencil-pen" "x-scheme-handler/openpencil"];
      })
    ];
    passthru = {
      inherit frontend cli;
      upstreamVersion = "0.15.1";
    };
    meta = {
      description = "OpenPencil design editor with document-safe MCP exports";
      mainProgram = "openpencil-desktop";
      platforms = ["x86_64-linux"];
      license = lib.licenses.mit;
    };
  }
