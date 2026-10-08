{pkgs, ...}: {
  programs.gofmt = {
    enable = true;
    package = pkgs.buildPackages.go_1_27;
  };
}
