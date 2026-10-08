# mkHomebrewFormula

`mkHomebrewFormula` generates a Homebrew formula (`.rb`) for packaging pre-built binary release archives. At Nix evaluation time, use the `":no_check"` placeholder when the archives do not exist yet. After the release archives are built, use [`rs-harbor brew bump`](./homebrew-cli.md) to compute real sha256 sums and update the tap.

The helper renders OS and CPU-specific downloads as nested Homebrew platform blocks, such as `on_macos do` with `on_arm do` or `on_intel do` inside it. A `sha256` value of `":no_check"` is emitted as Homebrew's `sha256 :no_check` symbol for workflows that fill checksums later.

## Parameters

- `pkgs`
- `name` — Homebrew formula name, e.g. `modde`
- `version` — release version without a leading `v`
- `description` — one-line description, 80 characters or fewer
- `homepage` — HTTPS project URL
- `license` — SPDX license identifier
- `platforms` — attrset keyed by `darwin_arm`, `darwin_intel`, `linux_arm`, or `linux_intel`; each value is `{ url; sha256; }`
- `dependencies` — list of Homebrew formula dependencies
- `binaries` — list of binaries installed into `bin`
- `caveats` — optional post-install message
- `testBlock` — optional Ruby body for `test do`
- `extraRubyBody` — optional raw Ruby appended to the formula body

## Example

```nix
packages.homebrew-formula = (rs-harbor.lib.mkHomebrewFormula {
  inherit pkgs;
  name = "my-app";
  version = "1.0.0";
  description = "My example application";
  homepage = "https://example.com/my-app";
  license = "MIT";
  platforms = {
    darwin_arm = {
      url = "https://example.com/releases/my-app-1.0.0-aarch64-darwin.tar.gz";
      sha256 = ":no_check";
    };
    darwin_intel = {
      url = "https://example.com/releases/my-app-1.0.0-x86_64-darwin.tar.gz";
      sha256 = ":no_check";
    };
  };
  dependencies = ["openssl@3"];
  binaries = ["my-app"];
  testBlock = ''system "#{bin}/my-app", "--version"'';
}).formulaPath;
```

The returned attrset also includes `formulaText` for inspection or for downstream tools that need to rewrite placeholders before publishing.
