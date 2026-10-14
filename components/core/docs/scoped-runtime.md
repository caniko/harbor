# Command-scoped development runtimes

`lib.core.devShell.mkRuntime` supplies loader libraries, executable paths and
environment values to one command and its children. The parent development
shell remains suitable for launching installed desktop applications.

```nix
runtime = harbor.lib.core.devShell.mkRuntime {
  inherit pkgs;
  name = "project-runtime";
  libraryPackages = [ pkgs.zlib ];
  pathPackages = [ pkgs.ffmpeg ];
  defaultEnv.VK_DRIVER_FILES = "${driver}/share/vulkan/icd.d/example.json";
};
```

Run `project-runtime <command> [arguments...]`. Library paths are prefixed on
Linux only; an empty library list leaves `LD_LIBRARY_PATH` untouched. Executable
paths are prefixed on every platform. `defaultEnv` fills unset or empty values,
while `env` sets values unconditionally. Arguments are preserved and `exec`
forwards the command's exit status and signal handling. Invocation without a
command exits with status 64. Variable names are validated and values are quoted.

GPU driver profiles belong to consumers. Pass their driver variables to the
runtime instead of exporting them across the development shell.

## Shell composition

`lib.core.devShell.mkShell { shells = [ rustShell pythonShell ]; ... }` inherits
the children's actual native/build/propagated dependencies. Child environment
metadata comes from `passthru.devShellSpec.env`, falling back to `env` for
external shells. Supply other external-shell variables explicitly.

Environment precedence follows child order, then fragment order, then the final
consumer `env`. Hooks execute in that same order, separated by newlines. Each
child's actual `shellHook` runs once. This is intentionally different from
`pkgs.mkShell.inputsFrom`, which executes child hooks in reverse order and does
not inherit child environment variables. Custom builders must forward
`spec.inputsFrom` alongside packages, environment and hooks.
