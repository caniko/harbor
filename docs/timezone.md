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

`withShell { inherit pkgs; shell = existingShell; timeZone = "Europe/Istanbul"; }`
adapts an existing derivation, including one from an older pinned Harbor. It
updates the exported environment and validates the zone at shell entry. When
`timeZone` is omitted, it inherits the shell's zone or defaults to UTC.

All shells composed with `lib.devShell.mkShell` include this wiring. Pass
`timeZone = "America/New_York"`; an existing `env.TZ` override remains
authoritative and is validated. An explicit `env.TZDIR` can select a different
database and is validated against the selected zone. Rust, Python, Node/Bun,
Go, TeX, Ethereum, Solana, NTT and Android shell APIs forward `timeZone`.

Harbor RS archives accept `archiveTimezone`, defaulting to UTC. ZIP uses local
time fields, so its files are normalized to 1980-01-01 in the configured zone.
Tar uses the timezone-independent Unix epoch. Both stay reproducible for fixed
inputs, regardless of the invoking machine's timezone.
