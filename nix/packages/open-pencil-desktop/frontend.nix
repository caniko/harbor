{
  lib,
  stdenv,
  stdenvNoCC,
  callPackage,
  bun,
  nodejs,
  cacert,
}: let
  src = callPackage ./source.nix {};
  modules = stdenvNoCC.mkDerivation {
    pname = "open-pencil-node-modules";
    version = "0.15.1";
    inherit src;
    nativeBuildInputs = [bun cacert];
    dontBuild = true;
    # Keep the relocatable workspace links and original shebangs inside an
    # archive; fixup runs only on the eventual application closure.
    dontFixup = true;
    installPhase = ''
      bun test ${./.}/normalize-links.test.ts
      # Independent cold installs must agree before accepting the fixed hash.
      # Bun's alias/bin races are repaired from the frozen graph, not by
      # dropping entries or choosing whichever install finished first.
      for attempt in 1 2; do
        work="$TMPDIR/install-$attempt"
        mkdir -p "$work" "$TMPDIR/home-$attempt"
        tar -xf ${src} -C "$work" --strip-components=1
        (
          cd "$work"
          export HOME="$TMPDIR/home-$attempt"
          bun install --frozen-lockfile --ignore-scripts --cache-dir "$TMPDIR/cache-$attempt"
          bun ${./normalize-links.ts}
          if [ "$attempt" = 1 ]; then
            bun ${./audit-dependencies.ts}
          fi
          # Bun's backend can share regular-file inodes differently across
          # filesystems. Store their bytes independently; workspace/dependency
          # symlinks remain links. Avoid host passwd/group names in headers.
          find . -type d -name node_modules -prune -print0 | sort -z |
            tar --null -T - --sort=name --mtime=@1 --owner=0 --group=0 \
              --numeric-owner --hard-dereference -cf "$TMPDIR/modules-$attempt.tar"
          if [ "$attempt" = 1 ]; then
            printf 'DEPENDENCY_ARCHIVE '
            sha256sum "$TMPDIR/modules-$attempt.tar"
            tar -tvf "$TMPDIR/modules-$attempt.tar" |
              awk '{ modes[$1]++; owners[$2]++ } END { for (mode in modes) print "ARCHIVE_MODE", mode, modes[mode]; for (owner in owners) print "ARCHIVE_OWNER", owner, owners[owner] }' |
              sort
          fi
        )
      done
      cmp "$TMPDIR/modules-1.tar" "$TMPDIR/modules-2.tar"
      mkdir -p $out
      cp "$TMPDIR/modules-1.tar" "$out/node_modules.tar"
    '';
    outputHashMode = "recursive";
    outputHashAlgo = "sha256";
    outputHash = "sha256-Dc91Uci/Wr5ZJS/rudvljjcFec7LZTmpH/czRaxw9kk=";
  };
in
  stdenvNoCC.mkDerivation {
    pname = "open-pencil-frontend";
    version = "0.15.1-canix1";
    inherit src;
    patches = [./document-export.patch];
    nativeBuildInputs = [bun nodejs];
    env.LD_LIBRARY_PATH = lib.makeLibraryPath [stdenv.cc.cc.lib];
    buildPhase = ''
      export HOME=$TMPDIR/home
      mkdir -p "$HOME"
      tar -xf ${modules}/node_modules.tar
      patchShebangs node_modules
      bun run build:packages
      bun run generate:icons --target desktop
      bun run generate:tauri-menu
      bunx --no-install vite build
    '';
    doCheck = true;
    checkPhase = ''
      bun test tests/engine/app/document/export.test.ts
    '';
    installPhase = ''
      mkdir -p $out
      cp -r dist desktop/icons desktop/generated $out/
    '';
  }
