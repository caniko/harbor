{
  pkgs,
  dioxusCli ? pkgs.dioxus-cli,
}:
pkgs.writeShellApplication {
  name = "dioxus-link-assets";
  text = ''
    if [ "$#" -ne 2 ]; then
      echo "usage: dioxus-link-assets EXECUTABLE DESTINATION" >&2
      exit 64
    fi

    executable=$1
    destination=$2

    if [ ! -f "$executable" ]; then
      echo "dioxus-link-assets: executable not found: $executable" >&2
      exit 66
    fi

    if [ ! -w "$executable" ]; then
      echo "dioxus-link-assets: executable is not writable: $executable" >&2
      exit 73
    fi

    mkdir -p "$destination"
    exec ${dioxusCli}/bin/dx tools assets "$executable" "$destination"
  '';
  meta.description = "Link Dioxus assets into an externally built server executable";
}
