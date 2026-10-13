{
  description = "Harbor shared build infrastructure: independent components in one repository";

  inputs = {
    # Consume qualified tool sources without their consumer flake graphs.
    simit = {
      url = "git+https://github.com/caniko/simit.git?ref=feat/harbor-monorepo-components&rev=db0883dc27476235f37f5e6f0be6b214648c5302";
      flake = false;
    };
    plinth-source = {
      url = "git+https://github.com/caniko/plinth.git?ref=fix/dioxus-0.7.10&rev=4718cc09d9d1b54cc10e23e60c37b656bf7285a6";
      flake = false;
    };
    # CAD's minimum FreeCAD version establishes the shared package-set floor.
    nixpkgs.url = "github:NixOS/nixpkgs/73e728ddb6b7a12d18808f510813a13ee1fe4cce";
    # Retained Intel macOS compatibility fixes qualified this maintained revision.
    nixpkgs-darwin.url = "github:NixOS/nixpkgs/2bd3427b41d10b8318383195efe502ed1baca6cd";
    crane.url = "github:ipetkov/crane/eb35abda9f232cc6610b1d1e3200d15c49b7ac54";
    flake-parts.url = "github:hercules-ci/flake-parts/31729ca8cbdb4fa927b34e5f4353e6a83f39e993";
    flake-utils.url = "github:numtide/flake-utils/11707dc2f618dd54ca8739b309ec4fc024de578b";
    rust-overlay = {
      url = "github:oxalica/rust-overlay/c1cfa7a2c809d364ac1a7e9b3ec418afa3f19793";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    treefmt-nix = {
      url = "github:numtide/treefmt-nix/27b3b12a8e6375f28ebe122f07d230ca5459bbfa";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    home-manager = {
      url = "github:nix-community/home-manager/0560d64401a6d5370732a69d08bc4a6b135962d9";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    git-hooks = {
      url = "github:cachix/git-hooks.nix/59f4ca0d063a1a3ec722c88b51a33004862e5379";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    osxcross = {
      url = "github:caniko/osxcross/246fc035534167fadba187a602bc4a40990974dc";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nix-bundle = {
      url = "github:nix-community/nix-bundle/eff01593f62794d458ec714090091419194ab64d";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nix-opencode-lsp = {
      url = "git+https://github.com/caniko/nix-opencode-lsp.git?ref=trunk&rev=b158ee4c3cc9c0d6ca7ab2216906b51990699c8a";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
    };
    pyproject-nix = {
      url = "github:pyproject-nix/pyproject.nix/7af23cfe91064865ecf2e835da28b45b3c6f49fd";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    uv2nix = {
      url = "github:pyproject-nix/uv2nix/2f698e4b5a3c6004edaf051543542f18a36afe77";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.pyproject-nix.follows = "pyproject-nix";
    };
    pyproject-build-systems = {
      url = "github:pyproject-nix/build-system-pkgs/430680a19bc85a3bda55f12e4cc1a1aadcf2e478";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.pyproject-nix.follows = "pyproject-nix";
      inputs.uv2nix.follows = "uv2nix";
    };
    bun-overlay = {
      url = "github:alleneubank/bun-overlay/7705bf47b3202134508da10a7b722b5529f28aab";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    fleetix = {
      url = "github:caniko/fleetix/2230d9ee804a66d94424a91919182e4fcca13ab2";
      flake = false;
    };
    # The SBF workspace manifest and lock are evaluation inputs, so use an
    # immutable source input rather than reading a fetch derivation (IFD).
    solana-source = {
      url = "github:anza-xyz/agave/14786d96e9b635fd5d57e503f3fa0397a8ef9a6d";
      flake = false;
    };
    solana-source-darwin = {
      url = "github:anza-xyz/agave/5a06890206cf9f00a5fbd253b8f417cc5c3a075c";
      flake = false;
    };
    openlb = {
      url = "git+https://gitlab.com/openlb/release.git?ref=1.9.0&rev=145cd54810b468f4b6fd3ed86b10644264841578";
      flake = false;
    };
  };

  outputs = inputs: import ./nix/outputs.nix inputs;
}
