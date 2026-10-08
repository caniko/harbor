{lib}: let
  schema = "https://opencode.ai/config.json";

  rustLsp = {
    rust.command = ["rust-analyzer"];
    nixd.command = ["nixd"];
    taplo = {
      command = ["taplo" "lsp" "stdio"];
      extensions = [".toml"];
    };
  };

  pythonLsp = {
    pyright.disabled = true;
    basedpyright = {
      command = ["basedpyright-langserver" "--stdio"];
      extensions = [".py" ".pyi"];
    };
    ruff = {
      command = ["ruff" "server"];
      extensions = [".py" ".pyi"];
    };
  };

  lspForKind = kind:
    if kind == "rust"
    then rustLsp
    else if kind == "python"
    then pythonLsp
    else if kind == "mixed"
    then rustLsp // pythonLsp
    else throw "meta-harbor.opencode: unsupported kind `${kind}`";

  configForKind = kind: {
    "$schema" = schema;
    lsp = lspForKind kind;
  };
in rec {
  inherit schema rustLsp pythonLsp lspForKind configForKind;

  supportedKinds = ["rust" "python" "mixed"];

  configText = kind:
    builtins.toJSON (configForKind kind) + "\n";

  configPath = ".opencode/opencode.jsonc";

  packagesForKind = pkgs: kind:
    if kind == "rust"
    then [pkgs.nixd pkgs.taplo]
    else if kind == "python"
    then [pkgs.basedpyright pkgs.ruff]
    else if kind == "mixed"
    then [pkgs.nixd pkgs.taplo pkgs.basedpyright pkgs.ruff]
    else throw "meta-harbor.opencode: unsupported kind `${kind}`";

  checkKind = kind:
    lib.assertMsg (builtins.elem kind supportedKinds)
    "meta-harbor.opencode: kind must be one of ${lib.concatStringsSep ", " supportedKinds}, got `${kind}`";
}
