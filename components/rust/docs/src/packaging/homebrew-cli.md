# Homebrew CLI

`harbor-rs brew bump` renders a Homebrew formula from release metadata and archive files that already exist on disk. It computes sha256 sums from the local archive paths and writes a ready-to-commit formula to a tap repository.

The archive URL is embedded in the formula, while the archive path is only used for hashing. The URL must point at the same bytes as the local path; otherwise `brew install` will fail with a checksum mismatch.

## Example

```bash
harbor-rs brew bump \
  --tap ../homebrew-tap \
  --name my-app \
  --version 1.0.0 \
  --description "My example application" \
  --homepage https://example.com/my-app \
  --license MIT \
  --depends openssl@3 \
  --binary my-app \
  --archive darwin_arm=https://example.com/releases/my-app-1.0.0-aarch64-darwin.tar.gz,dist/my-app-darwin-arm.tar.gz \
  --archive linux_intel=https://example.com/releases/my-app-1.0.0-x86_64-linux.tar.gz,dist/my-app-linux-intel.tar.gz
```

Use `--stdout` instead of `--tap` to print the formula for review or piping:

```bash
harbor-rs brew bump --stdout \
  --name my-app \
  --version 1.0.0 \
  --description "My example application" \
  --homepage https://example.com/my-app \
  --license MIT \
  --archive linux_intel=https://example.com/my-app.tar.gz,dist/my-app.tar.gz
```

Use `--push` to run `git add`, `git commit`, and `git push` in the tap repository. This mode is default-off, requires `--tap`, and aborts if the tap has dirty files other than `Formula/<name>.rb`. Git credentials are the caller's responsibility.
