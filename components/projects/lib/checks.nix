{...}: {
  # Fails when a docs/src/*.md file is not referenced from SUMMARY.md.
  # src is the consumer's docs/ dir.
  mkSummaryCheck = {
    pkgs,
    src,
    name ? "docs-summary",
  }:
    pkgs.runCommand name {
      inherit src;
    } ''
      missing=0
      while IFS= read -r -d "" file; do
        base="$(basename "$file")"
        if ! grep -q -- "$base" "$src/src/SUMMARY.md"; then
          echo "docs-summary: $file not referenced from SUMMARY.md" >&2
          missing=1
        fi
      done < <(find "$src/src" -name "*.md" ! -name "SUMMARY.md" -print0)
      if [ "$missing" -ne 0 ]; then
        exit 1
      fi
      mkdir -p "$out"
    '';

  # Landing/funnel contract, generalized from pink-raven's websiteMarkers:
  # the built site must mention its title, link the embedded app URL
  # path, and contain the given rendered section markers. appRoute is
  # the href form ("/<appDir>"); extraGreps adds consumer-specific
  # `grep -q -- '<pattern>' ${website}/...` lines.
  mkWebsiteMarkers = {
    pkgs,
    website,
    title,
    appRoute ? "/app",
    sections ? ["workflow-steps" "audience-grid" "trust-panel"],
    extraGreps ? [],
    name ? "website-markers",
  }:
    pkgs.runCommand name {} ''
      grep -q ${pkgs.lib.escapeShellArg title} ${website}/index.html
      grep -q ${pkgs.lib.escapeShellArg "href=\"${appRoute}\""} ${website}/index.html
      ${pkgs.lib.concatMapStringsSep "\n" (s: "grep -q ${pkgs.lib.escapeShellArg s} ${website}/index.html") sections}
      ${pkgs.lib.concatStringsSep "\n" extraGreps}
      touch $out
    '';
}
