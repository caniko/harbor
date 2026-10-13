# harbor-cache

Reusable Attic helpers for Harbor and downstream Rust tooling: JWT payload
decoding and expiry reporting, cache push invocation and administrator token
issuance. Commands invoke the caller-supplied Attic executable.

JWT decoding does **not** verify signatures or authorize access. Expiry checks
take an explicit clock value so consumers can test policy deterministically.

```rust
use harbor_cache::{JwtPayload, JwtStatus, status};
use std::time::Duration;

let payload = JwtPayload {
    exp: 2_000,
    nbf: 1_000,
    sub: "example-reader".into(),
    attic_scopes: serde_json::Value::Null,
};
assert_eq!(status(&payload, 3_000), JwtStatus::Expired {
    ago: Duration::from_secs(1_000),
});
```

See the [API documentation](https://docs.rs/harbor-cache) and the
[Harbor repository](https://github.com/caniko/harbor) for Nix integration.
Rust 1.88 is supported. The crate requires no Canix inventory or credentials.

Licensed under MIT OR Apache-2.0; both texts are included in the crate.
