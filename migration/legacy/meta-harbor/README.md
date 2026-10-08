# meta-harbor

Shared harbor helpers for editor and agent tooling.

`meta-harbor` owns cross-language policy that is useful to both `rs-harbor`
and `py-harbor`. The first shared surface is opencode LSP discovery: checked-in
`.opencode/opencode.jsonc` files declare the LSPs opencode may use, while each
project's direnv-powered harbor dev shell supplies the actual binaries.

## Usage

```bash
harbor-opencode sync --kind detect
harbor-opencode check --kind detect
harbor-opencode rollout --root /data/nvme0/can/Projects
```
