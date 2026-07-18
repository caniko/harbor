{
  nixLib,
  mkTexlive,
}: {
  mkLatexDocument = {
    pkgs,
    name,
    src,
    mainFile,
    engine ? "lualatex",
    profile ? "article",
    shellEscape ? false,
    extraTexPackages ? (_: []),
    nativeBuildInputs ? [],
    latexmkArgs ? [],
    outputName ? "${name}.pdf",
  }: let
    texlive = mkTexlive {
      inherit pkgs profile;
      extraPackages = ps: [ps.latexmk] ++ extraTexPackages ps;
    };
    engineArgSets = {
      pdflatex = ["-pdf"];
      lualatex = ["-pdf" "-lualatex"];
    };
    engineArgs =
      if builtins.hasAttr engine engineArgSets
      then engineArgSets.${engine}
      else throw "tex-harbor: unsupported LaTeX engine `${engine}`";
    allArgs =
      engineArgs
      ++ ["-interaction=nonstopmode" "-halt-on-error" "-file-line-error"]
      ++ nixLib.optional shellEscape "-shell-escape"
      ++ latexmkArgs;
    commandArgs = builtins.concatStringsSep " " (map nixLib.escapeShellArg allArgs);
    mainArg = nixLib.escapeShellArg mainFile;
    pdfFile = "${nixLib.removeSuffix ".tex" mainFile}.pdf";
  in
    assert nixLib.assertMsg (!nixLib.hasInfix ".." mainFile) "tex-harbor: mainFile must stay below src";
    assert nixLib.assertMsg (nixLib.hasSuffix ".tex" mainFile) "tex-harbor: mainFile must end in .tex";
    assert nixLib.assertMsg (nixLib.baseNameOf outputName == outputName) "tex-harbor: outputName must be a file name";
      pkgs.stdenvNoCC.mkDerivation {
        pname = name;
        version = "0.1.0";
        inherit src;

        nativeBuildInputs = [texlive] ++ nativeBuildInputs;
        dontConfigure = true;

        buildPhase = ''
          runHook preBuild
          buildRoot="$TMPDIR/tex-harbor-build"
          mkdir -p "$buildRoot" "$TMPDIR/home" "$TMPDIR/texmf-var" "$TMPDIR/texmf-config"
          cp -R --no-preserve=mode "$src"/. "$buildRoot"/
          cd "$buildRoot"
          export HOME="$TMPDIR/home"
          export TEXMFVAR="$TMPDIR/texmf-var"
          export TEXMFCONFIG="$TMPDIR/texmf-config"
          latexmk ${commandArgs} ${mainArg}
          runHook postBuild
        '';

        installPhase = ''
          cd "$TMPDIR/tex-harbor-build"
          mkdir -p "$out"
          test -f ${nixLib.escapeShellArg pdfFile}
          install -Dm644 ${nixLib.escapeShellArg pdfFile} "$out/${outputName}"
        '';
      };
}
