{
  pkgs,
  devShell,
}: let
  runtime = devShell.mkRuntime {
    inherit pkgs;
    name = "runtime-probe";
    libraryPackages = [pkgs.zlib];
    pathPackages = [pkgs.hello];
    env.PROBE_FIXED = "a value with ' quotes";
    defaultEnv.PROBE_DEFAULT = "default value";
  };
  empty = devShell.mkRuntime {
    inherit pkgs;
    name = "empty-runtime";
  };
  child = pkgs.mkShell {
    buildInputs = [pkgs.zlib];
    propagatedBuildInputs = [pkgs.bzip2];
    propagatedNativeBuildInputs = [pkgs.jq];
    shellHook = "export PROBE_ORDER=child";
    passthru.devShellSpec.env.PROBE_ENV = "child";
  };
  composed = devShell.mkShell {
    inherit pkgs;
    shells = [child];
    fragments = [{shellHook = "export PROBE_ORDER=\"$PROBE_ORDER,fragment\"";}];
    env.PROBE_ENV = "parent";
    extraShellHook = "export PROBE_ORDER=\"$PROBE_ORDER,parent\"";
  };
in
  assert composed.PROBE_ENV == "parent";
  assert pkgs.lib.all (input: builtins.elem input composed.buildInputs) child.buildInputs;
  assert pkgs.lib.all (input: builtins.elem input composed.propagatedBuildInputs) child.propagatedBuildInputs;
  assert pkgs.lib.all (input: builtins.elem input composed.propagatedNativeBuildInputs) child.propagatedNativeBuildInputs;
    pkgs.runCommand "harbor-meta-scoped-runtime" {
      nativeBuildInputs = [pkgs.python3];
      env = composed.passthru.devShellSpec.env;
    } ''
      unset LD_LIBRARY_PATH PROBE_DEFAULT PROBE_FIXED
      ${runtime}/bin/runtime-probe python - <<'PY'
      import os, shutil
      assert os.environ["PROBE_FIXED"] == "a value with ' quotes"
      assert os.environ["PROBE_DEFAULT"] == "default value"
      assert shutil.which("hello") == "${pkgs.hello}/bin/hello"
      ${pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''assert os.environ["LD_LIBRARY_PATH"] == "${pkgs.lib.makeLibraryPath [pkgs.zlib]}"''}
      PY
      PROBE_DEFAULT=caller ${runtime}/bin/runtime-probe python -c 'import os; assert os.environ["PROBE_DEFAULT"] == "caller"'
      ${empty}/bin/empty-runtime python -c 'import os; assert "LD_LIBRARY_PATH" not in os.environ'
      ${runtime}/bin/runtime-probe python -c 'import sys; assert sys.argv[1:] == ["a b", "", "--flag"]' 'a b' "" --flag
      if ${runtime}/bin/runtime-probe python -c 'raise SystemExit(37)'; then exit 1; else test "$?" = 37; fi
      test -z "''${LD_LIBRARY_PATH:-}"
      test -z "''${PROBE_FIXED:-}"
      ${composed.shellHook}
      test "$PROBE_ORDER" = child,fragment,parent
      touch "$out"
    ''
