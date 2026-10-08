{pkgs}:
# Dioxus' workspace wrapper captures linker and asset arguments, but older
# releases invoke rustc directly.  Keep the capture layer and delegate the
# compiler invocation to rs-harbor's sccache wrapper when requested.
pkgs.dioxus-cli.overrideAttrs (old: {
  patches = (old.patches or []) ++ [./../patches/dioxus-cli-inner-wrapper.patch];
})
