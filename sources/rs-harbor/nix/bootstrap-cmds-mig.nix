# bootstrapCmdsMig :: pkgs -> derivation
#
# Cross-platform fork of Apple's bootstrap_cmds, providing `mig` (Mach
# Interface Generator). Required by `mig`-using crates (e.g. several of
# the system-configuration-sys / security-framework-sys ecosystem) when
# cross-compiling to Darwin from non-Darwin hosts via osxcross.
{pkgs}:
pkgs.stdenv.mkDerivation {
  pname = "bootstrap-cmds-mig";
  version = "cross-platform-3cc6b1c";

  src = pkgs.fetchFromGitHub {
    owner = "markmentovai";
    repo = "bootstrap_cmds";
    rev = "3cc6b1cf291f8fccfbf6444d6630a02a54c16831";
    sha256 = "1ns32nq91n9f5la7vn7ksb5ljfj8spq3c5vnfqz7vccyzn297ll6";
  };

  nativeBuildInputs = with pkgs; [
    autoreconfHook
    bison
    flex
  ];

  env.CFLAGS = "-std=gnu89";

  enableParallelBuilding = true;

  meta = {
    description = "Cross-platform Mach Interface Generator (markmentovai/bootstrap_cmds)";
    homepage = "https://github.com/markmentovai/bootstrap_cmds";
    license = pkgs.lib.licenses.apsl20;
    platforms = pkgs.lib.platforms.unix;
  };
}
