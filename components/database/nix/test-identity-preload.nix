{pkgs}:
pkgs.runCommand "harbor-db-identity-preload-fixture" {nativeBuildInputs = [pkgs.stdenv.cc];} ''
  mkdir -p "$out/lib"
  cc -shared -fPIC -o "$out/lib/identity.so" ${../tests/fixtures/identity_preload.c} -ldl
''
