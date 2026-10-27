let
  portable = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];
  linux = ["x86_64-linux" "aarch64-linux"];
  tool = frameworks: provides: {
    inherit frameworks provides;
    systems = portable;
    optional = false;
  };
  linuxTool = frameworks: provides:
    (tool frameworks provides)
    // {
      systems = linux;
      optional = true;
    };
in {
  frameworks = {
    core = "Shared Harbor, Nix and formatting tooling";
    rust = "Rust compiler, analyzer and Harbor build helpers";
    python = "Python, uv and Python editor tooling";
    javascript = "Node, pnpm, Bun and UI design tooling";
    go = "Go, gopls and golangci-lint";
    android = "Explicitly versioned Android SDK/NDK and device tooling";
    tex = "Curated TeX profile, editor and article plugins";
    projects = "Documentation, site and UI design tooling";
    database = "Harbor database and lifecycle executables";
    cad = "Harbor CAD, FreeCAD, Blender and their MCP companions";
    llm = "Harbor project-environment harness integrations";
    evm = "Foundry and Solidity";
    solana = "Anchor, Agave and SBF tooling";
    ntt = "Consumer-pinned Wormhole NTT toolchain";
    macos-sdk = "macOS SDK validation and realization tooling";
  };
  tools = {
    harbor-opencode = tool ["core"] ["harbor-opencode"];
    nixd = tool ["core"] ["nixd"];
    taplo = tool ["core"] ["taplo"];
    treefmt = tool ["core"] ["treefmt"];
    rust = tool ["rust" "solana" "android"] ["cargo" "rustc" "rust-analyzer" "rustfmt" "clippy-driver"];
    harbor-rs = tool ["rust"] ["harbor-rs"];
    harbor-ci = tool ["rust"] ["harbor-ci"];
    sccache = tool ["rust"] ["sccache"];
    python = tool ["python"] ["python" "python3"];
    uv = tool ["python"] ["uv" "uvx"];
    ruff = tool ["python"] ["ruff"];
    pyright = tool ["python"] ["pyright" "pyright-langserver"];
    node = tool ["javascript" "projects"] ["node" "npm" "npx"];
    pnpm = tool ["javascript" "projects"] ["pnpm" "pnpx"];
    bun = tool ["javascript" "projects"] ["bun" "bunx"];
    typescript = tool ["javascript"] ["typescript-language-server"];
    go = (tool ["go"] ["go" "gofmt"]) // {systems = ["x86_64-linux" "aarch64-linux" "aarch64-darwin"];};
    gopls = (tool ["go"] ["gopls"]) // {systems = ["x86_64-linux" "aarch64-linux" "aarch64-darwin"];};
    golangci-lint = (tool ["go"] ["golangci-lint"]) // {systems = ["x86_64-linux" "aarch64-linux" "aarch64-darwin"];};
    android = tool ["android"] ["adb" "sdkmanager"];
    android-device = tool ["android"] ["android-device"];
    cargo-ndk = tool ["android"] ["cargo-ndk"];
    jdk = tool ["android"] ["java" "javac"];
    gradle = tool ["android"] ["gradle"];
    texlive = tool ["tex"] ["latexmk" "lualatex" "pdflatex"];
    texlab = tool ["tex"] ["texlab"];
    pandoc = tool ["tex" "projects"] ["pandoc"];
    anx-pandoc = tool ["tex"] ["anx-plugin-pandoc"];
    anx-zenodo = tool ["tex"] ["anx-plugin-zenodo"];
    mdbook = tool ["projects"] ["mdbook"];
    database = (linuxTool ["database"] ["harbor-db" "home-manager-backup"]) // {optional = false;};
    foundry = tool ["evm"] ["forge" "cast" "anvil" "chisel"];
    solc = tool ["evm"] ["solc"];
    anchor = tool ["solana"] ["anchor"];
    solana = tool ["solana"] ["solana"];
    cargo-build-sbf = tool ["solana"] ["cargo-build-sbf"];
    ntt = (tool ["ntt"] ["ntt" "bun" "bunx" "forge" "cast" "anvil" "chisel" "solc" "anchor" "node" "npm" "npx" "solana" "cargo" "rustc"]) // {systems = ["x86_64-linux"];};
    llm = (linuxTool ["llm"] []) // {optional = false;};
    cad = (tool ["cad"] ["harbor-cad"]) // {systems = ["x86_64-linux"];};
    freecad = (tool ["cad"] ["FreeCAD" "FreeCADCmd"]) // {systems = ["x86_64-linux"];};
    blender = tool ["cad"] ["blender"];
    openpencil = tool ["javascript" "projects"] ["openpencil" "openpencil-mcp" "openpencil-mcp-http"];
    macos-sdk = tool ["macos-sdk"] ["validate-macos-sdk"];
  };
  # Skill content lives in `owner`'s component; `requires` declares the
  # frameworks it needs, optionally narrowed to `tools` (realized into the
  # skill's env/bin; default: every tool of those frameworks), and extra TeX
  # packages.
  skills.dataset-report = {
    owner = "llm";
    source = "skills/dataset-report";
    requires.frameworks = ["tex" "python"];
    requires.tools = ["texlive" "python"];
    requires.tex.extraPackages = ["pgfplots"];
  };
}
