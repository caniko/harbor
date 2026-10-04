# harbor-go

Reusable Go toolchains, reproducible module builds, and composable development
shells for Nix flakes. The default toolchain is Go 1.27.1, gopls 0.23.0, and
golangci-lint 2.13.1. The compiler is selected explicitly: Nixpkgs' default Go
is an older minor version.

Native outputs support `x86_64-linux`, `aarch64-linux`, and `aarch64-darwin`.
Current Nixpkgs no longer supports native `x86_64-darwin` environments.

## Start a project

```sh
nix flake init -t github:caniko/harbor-go/trunk
nix develop
go test ./...
```

The template provides `packages.default`, package and formatting checks,
`devShells.default`, and `formatter`. Commit the generated consumer lock file. Published consumers
should lock a qualified Harbor revision before building.

## Public API

All helpers take a consumer-owned `pkgs` argument; Harbor does not introduce a
second package set or a global overlay.

| Helper | Contract |
| --- | --- |
| `mkGoToolchain { pkgs; go?; gopls?; golangciLint?; }` | Returns `go`, `gopls`, `golangciLint`, and `buildGoModule`. Default tools are built with the selected compiler. |
| `mkGoDevShellFragment { pkgs; toolchain?; cgo?; }` | Returns `{ packages, env, shellHook }` for `harbor-meta.lib.devShell.mkShell`. CGO defaults to enabled with a C compiler and pkg-config. |
| `mkGoDevShell { pkgs; toolchain?; cgo?; packages?; env?; extraShellHook?; mkShellArgs?; }` | Convenience shell; extra environment values override the fragment defaults. Use `mkShellArgs.buildInputs` for native libraries. |
| `mkGoPackage { pkgs; toolchain?; ... }` | Delegates remaining arguments to the toolchain's Nixpkgs `buildGoModule`. Sets `env.GOTOOLCHAIN = "local"` by default. |

`mkGoPackage` accepts the upstream builder's argument form, including attribute
sets for `env`, `vendorHash`, `proxyVendor`, `modRoot`, `subPackages`, `tags`,
`ldflags`, `checkFlags`, native dependencies, and phase hooks. Module download
is confined to the fixed-output dependency derivation; compilation and checks
use its vendored dependencies. Set a real `vendorHash` for external modules;
`null` is appropriate only for dependency-free or already-vendored sources.

`GOTOOLCHAIN=local` prevents Go from silently downloading a different compiler.
An older consumer compiler or package set must be updated deliberately. Custom
gopls/linter packages must support that compiler and the project's Go version.

```nix
let
  toolchain = harbor-go.lib.mkGoToolchain {
    inherit pkgs;
    go = pkgs.buildPackages.go_1_27;
  };
in harbor-go.lib.mkGoPackage {
  inherit pkgs toolchain;
  pname = "my-cli";
  version = "1.0.0";
  src = self;
  vendorHash = "sha256-...";
  subPackages = ["cmd/my-cli"];
  env.CGO_ENABLED = "0";
  ldflags = ["-X example.com/my-cli/internal/version.Version=1.0.0"];
}
```

Packages export `go` (also `default`), `gopls`, `golangci-lint`, and `example`.
`treefmtModules.go` enables gofmt from Go 1.27. Consumers can override its package
with the same compiler used by their toolchain. Run formatting through treefmt.
The repository's `.envrc` loads its default shell with nix-direnv.

## Roborev consumer recipe

Requirements were traced against kenn-io/roborev revision
`26a236d3eeb7ce3062cb8e30101dec3ac8018670`:

- `go.mod` requires Go 1.27.0; CI uses 1.27.1.
- `make lint-ci` requires golangci-lint **exactly 2.13.1**. Harbor retains
  Nixpkgs' linter packaging and pins that version's source and vendor hashes.
- CGO/race tests need a C compiler. Add SQLite headers to the consumer shell
  for upstream CGO checks, and Git for tests that invoke Git.
- Production builds use `CGO_ENABLED=0` and
  `-X go.kenn.io/roborev/internal/version.Version=<version>`.
- A successful plain Go build can embed a placeholder web page. The production
  package must include the validated Bun/Svelte distribution.

Compose the Go shell fragment with `harbor-js.lib.mkBunToolchain` using
`harbor-meta.lib.devShell.mkShell`. Add project tools such as Git, GNU Make,
SQLite, and PostgreSQL in the consumer. PostgreSQL tests use a disposable local
database and `TEST_POSTGRES_URL`; external agent/credential tests remain explicit
operator invocations.

For a reproducible production package:

1. Select Bun explicitly. `package.json` declares 1.3.14, while CI uses 1.4.2;
   the consumer must resolve this discrepancy against its selected source.
2. Fetch the root Bun workspace dependencies with
   `harbor-js.lib.mkBunWorkspaceDeps`, a frozen `bun.lock`, and a verified hash.
   This includes the pinned Git dependency `kenn-io/kit-ui`.
3. Build `web/dist` in a separate frontend derivation, run its asset validator,
   and retain the hidden `.vite/manifest.json` in the output.
4. In the Go derivation's **`postConfigure`**, replace `internal/web/dist` with
   that validated output and make the copied files writable for tests.
   Nixpkgs also runs `preBuild` and `postPatch` in the module-vendoring derivation,
   so asset assembly belongs in the final build's `postConfigure`.
5. Build `./cmd/roborev` with the linker version and CGO disabled. Verify the
   installed binary with `roborev verify-web-assets`; require this install check
   before accepting the production output.

API generation (`make api-check`) needs both Go and Bun. Its pinned `go run
...@version` tooling needs a separately fetched module cache for sandboxed
checks; the regular package's vendor tree alone is insufficient. Keep this
project-specific check in the roborev flake.

## Qualification

The checks build and test the consumer template, compile a CGO fixture under
the race detector, verify generated embedded assets and linker flags in a
pure-Go binary, inspect compiler versions of gopls and the linter, and validate
shell/template contracts with harbor-meta. GitHub CI is generated by Simit and
runs the checks on native runners for each supported system.

```sh
treefmt --fail-on-change
nix flake check --no-write-lock-file
simit init ci --platform github --runtime nix --check --diff
git diff --check
```

On Canix hosts, realize local packages through `canix cache binary build` and
opt into test-only outputs with `--include-tests`. CI uses the standard Nix
flake check command.
