{
  pkgs,
  buildGoModule,
}: let
  version = "2.13.1";
in
  # Roborev refuses other versions. Hashes originate from Nixpkgs
  # 93a175eed6efb4a41fbcee1f0c5a1da7e9a52ae9; retain its packaging hooks.
  (pkgs.golangci-lint.override {buildGo127Module = buildGoModule;}).overrideAttrs (old: {
    inherit version;
    src = pkgs.fetchFromGitHub {
      owner = "golangci";
      repo = "golangci-lint";
      tag = "v${version}";
      hash = "sha256-8nWHSMAwIILfKMPfxWKMimxWt9N+kUsZEAaoAOPbRBE=";
    };
    vendorHash = "sha256-yZRqfht5rY2yyoZNtYttE57sB7EYjk71yrKw8dLYzNk=";
    ldflags = ["-s" "-w" "-X main.version=${version}" "-X main.commit=v${version}" "-X main.date=1970-01-01T00:00:00Z"];
    meta = old.meta // {changelog = "https://github.com/golangci/golangci-lint/blob/v${version}/CHANGELOG.md";};
  })
