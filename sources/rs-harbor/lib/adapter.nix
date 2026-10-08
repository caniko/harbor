# mkAdapter        :: { attic } -> harbor-adapter
# isHarborAdapter  :: value -> bool
#
# Create a typed harbor adapter for binary cache configuration.
# Export from your NixOS/infra flake; pass to mkAtticPush in Rust projects.
#
# Usage (NixOS flake):
#   harborAdapter = harbor.lib.mkAdapter {
#     attic = { endpoint = "https://cache.myhost.com"; cache = "main"; };
#   };
rec {
  # Pure predicate — returns true if the value is a well-formed harbor adapter.
  isHarborAdapter = v:
    builtins.isAttrs v
    && v ? _type
    && v._type == "harbor-adapter"
    && v ? _version
    && builtins.isInt v._version
    && v ? attic
    && builtins.isAttrs v.attic
    && v.attic ? endpoint
    && builtins.isString v.attic.endpoint
    && v.attic ? cache
    && builtins.isString v.attic.cache;

  # Build a typed adapter attrset from raw configuration.
  #
  # Args:
  #   attic.endpoint   - Attic server URL (must start with http:// or https://)
  #   attic.cache      - cache name on the server
  #   attic.tokenEnvVar - env var the push script reads at runtime (default: "ATTIC_TOKEN")
  mkAdapter = {attic}:
    assert builtins.isAttrs attic
    || throw "rs-harbor: mkAdapter 'attic' must be an attrset";
    assert attic ? endpoint
    && builtins.isString attic.endpoint
    || throw "rs-harbor: mkAdapter 'attic.endpoint' is required and must be a string";
    assert attic ? cache
    && builtins.isString attic.cache
    || throw "rs-harbor: mkAdapter 'attic.cache' is required and must be a string";
    assert builtins.stringLength attic.endpoint
    > 0
    || throw "rs-harbor: mkAdapter 'attic.endpoint' must not be empty";
    assert builtins.stringLength attic.cache
    > 0
    || throw "rs-harbor: mkAdapter 'attic.cache' must not be empty";
    assert builtins.match "https?://.*" attic.endpoint
    != null
    || throw "rs-harbor: mkAdapter 'attic.endpoint' must start with http:// or https://, got \"${attic.endpoint}\"";
    assert !attic ? tokenEnvVar
    || builtins.isString attic.tokenEnvVar
    || throw "rs-harbor: mkAdapter 'attic.tokenEnvVar' must be a string if provided"; {
      _type = "harbor-adapter";
      _version = 1;
      attic = {
        inherit (attic) endpoint cache;
        tokenEnvVar = attic.tokenEnvVar or "ATTIC_TOKEN";
        serverName = "harbor";
      };
    };
}
