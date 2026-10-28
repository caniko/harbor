{
  pkgs,
  bundle,
  names,
}:
pkgs.runCommand "harbor-llm-skills-check" {} ''
  set -euo pipefail
  ${pkgs.lib.concatMapStringsSep "\n" (name: ''
      test "$(awk '/^---$/ {n++} n == 1 && sub(/^name: /, "") {print; exit}' ${bundle}/skills/${name}/SKILL.md)" = ${name}
    '')
    names}
  skill=${bundle}/skills/dataset-report
  cd "$skill"
  env/bin/python3 -I scripts/reconcile.py --self-test
  export HOME="$TMPDIR/home" TEXMFHOME="$TMPDIR/texmf-home" TEXMFVAR="$TMPDIR/texmf-var" TEXMFCONFIG="$TMPDIR/texmf-config"
  mkdir -p "$HOME" "$TMPDIR/build"
  cp assets/report.tex "$TMPDIR/build/"
  cd "$TMPDIR/build"
  "$skill/env/bin/latexmk" -lualatex -interaction=nonstopmode -halt-on-error report.tex
  test -s report.pdf
  touch "$out"
''
