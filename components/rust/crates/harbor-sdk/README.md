# caniko-harbor-sdk

Rust helpers for realizing, validating, and publishing macOS SDK archives with
Nix and Attic. The package belongs to the [Harbor workspace](https://github.com/caniko/harbor).

The crates.io package name is `caniko-harbor-sdk`; the Rust library remains
`harbor_sdk`. To preserve the dependency alias used by Harbor's CLI:

```toml
[dependencies]
harbor-sdk = { package = "caniko-harbor-sdk", version = "0.1.0" }
```

## API

`realize_macos_sdk(archive, version)` invokes `realize-macos-sdk --env`, parses
the resulting store path, SDK root, recursive hash, and version, then invokes
`validate-macos-sdk`. Both helper executables must be on `PATH`.

`publish_macos_sdk(PublishOpts)` realizes and validates the archive, optionally
registers a GC root, and invokes `attic push`. It reads the token from the
specified file and uses a temporary Attic configuration. The caller supplies
the archive, SDK version, server URL, cache name, token file, and optional root.
This operation requires the SDK helpers, `attic`, and, when rooting, `nix-store`.

`create_gc_root(store_path, link)` registers an indirect Nix GC root and creates
the link's parent directory as needed. All three functions return errors when
their required executables, files, command results, or parsed outputs are invalid.
The crate supplies orchestration; it does not bundle or grant rights to an SDK.

API documentation is published at <https://docs.rs/caniko-harbor-sdk> when the
package release is available. See `CHANGELOG.md` for the package history.

## Validation

From the Harbor workspace, use the approved development environment:

```sh
direnv exec . cargo test --locked -p caniko-harbor-sdk
direnv exec . cargo clippy --locked -p caniko-harbor-sdk --all-targets -- -D warnings
direnv exec . cargo doc --locked -p caniko-harbor-sdk --no-deps
direnv exec . cargo package --locked -p caniko-harbor-sdk
direnv exec . treefmt --ci
```

Package publication uses Harbor's signed package-qualified tag and protected
publisher workflows after component qualification. Package compilation alone
does not qualify real SDK realization, cache publication, or downstream use.

## License

MIT OR Apache-2.0. Both imported license texts are bundled with the package.
