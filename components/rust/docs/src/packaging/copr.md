# mkCoprSpec

`mkCoprSpec` generates a Fedora RPM `.spec` for packaging via [COPR](https://copr.fedorainfracloud.org/), Fedora's community build service. harbor-rs only produces the spec — uploading or building it stays in CI (or a shell), where the COPR credentials live.

The defaults assume the "binary-shipping" workflow: a binary already built in Nix is bundled as the `Source0` tarball, then installed to `%{_bindir}` by the generated `%install` section.

## Parameters

- `pkgs`
- `name`
- `version` — must not contain `-` (RPM forbids it in `Version:`)
- `release` — defaults to `1%{?dist}`
- `summary`
- `license`
- `url`
- `sources` — list of `Source*` lines; defaults to `Source0: %{name}-%{version}.tar.gz`
- `buildArch` — e.g. `"x86_64"` or `"noarch"`; omitted by default
- `buildRequires`
- `requires`
- `description` — defaults to `summary`
- `prep`, `build`, `install`, `files` — override the generated section bodies
- `desktopFile`, `icon`, `appId` — if `appId` (reverse-DNS) is set, the default `%install` and `%files` add `${appId}.desktop` and a 256x256 hicolor icon
- `changelog` — list of `{ date; author; version; entries; }`
- `coprMakefile` — when `true`, also emit a `.copr/Makefile` for COPR's custom-build SCM method
- `extraSections` — appended raw before `%changelog`

## Example

```nix
packages.copr-spec = (harbor-rs.lib.mkCoprSpec {
  inherit pkgs;
  name = "my-app";
  version = "1.0.0";
  summary = "My example application";
  license = "MIT";
  url = "https://example.com/my-app";
  appId = "com.example.MyApp";
  desktopFile = ''
    [Desktop Entry]
    Type=Application
    Name=My App
    Exec=my-app
    Icon=com.example.MyApp
    Categories=Utility;
  '';
  changelog = [
    {
      date = "Tue May 19 2026";
      author = "Can <can@example.com>";
      version = "1.0.0-1";
      entries = ["Initial COPR release"];
    }
  ];
}).specPath;
```

## Workflow

```bash
nix build .#my-app
nix build .#copr-spec
tar czf my-app-1.0.0.tar.gz -C result/bin my-app
copr-cli build my-project ./result
```

For a pre-built binary, you usually want to disable COPR's debuginfo extraction. Pass it through `extraSections`:

```nix
extraSections = "%global debug_package %{nil}";
```

If you'd rather have COPR drive the build itself from your git repo, set `coprMakefile = true` and commit the generated `.copr/Makefile` plus the `.spec` — then point a COPR "Custom" build at the repo.
