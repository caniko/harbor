# Timezone environments

`lib.timezone` owns cross-language timezone wiring. It defaults to UTC and
supplies both `TZ` and the Nix-packaged `TZDIR`; named timezones do not depend
on the host's `/etc/localtime` or `/usr/share/zoneinfo`.

```nix
let
  timezone = harbor-meta.lib.timezone.mkEnvironment {
    inherit pkgs;
    timeZone = "Europe/Istanbul";
  };
in pkgs.runCommand "example" timezone.env ''
  ${timezone.validationScript}
  date > "$out"
''
```

`mkEnvironment` returns `env`, `validationScript`, and a shell fragment
(`packages`, `env`, `shellHook`). `mkEnv` returns just the environment for
wrappers and builders. `tzdata` can be overridden with a compatible pinned
package containing `share/zoneinfo`. Use `validationScript` when accepting a
configured zone: invalid zone names fail at evaluation, and absent or non-TZif
zone files fail at shell entry or build time without import-from-derivation.
The shell hook exports the configured `TZ` before validation, since `nix develop`
omits that variable when restoring the build environment. A caller's inherited
`TZ` does not override the shell's configured zone. `validationScript` itself
only validates the current environment, so builders can check runtime overrides.

`withShell { inherit pkgs; shell = existingShell; timeZone = "Europe/Istanbul"; }`
adapts an existing derivation, including one from an older pinned Harbor. It
updates the exported environment and validates the zone at shell entry. When
`timeZone` is omitted, it inherits the shell's zone or defaults to UTC.
An explicit `TZDIR` in either the top-level attributes or `env` is preserved,
including in `devShellSpec`; the entry hook validates the selected zone against
that database.
Repeated adaptation replaces Harbor's entry prefix while retaining the user
hook, so changing a composed shell's zone does not run an older zone export.

All shells composed with `lib.devShell.mkShell` include this wiring. Pass
`timeZone = "America/New_York"`; an existing `env.TZ` override remains
authoritative and is validated. An explicit `env.TZDIR` can select a different
database and is validated against the selected zone. Rust, Python, Node/Bun,
Go, TeX, Ethereum, Solana, NTT and Android shell APIs forward `timeZone`.

Harbor RS archives accept `archiveTimezone`, defaulting to UTC. ZIP uses local
time fields, so its files are normalized to 1980-01-01 in the configured zone.
Tar uses the timezone-independent Unix epoch. Both stay reproducible for fixed
inputs, regardless of the invoking machine's timezone.
