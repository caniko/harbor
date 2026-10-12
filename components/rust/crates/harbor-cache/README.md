# harbor-cache

Reusable Attic helpers for Harbor and downstream Rust tooling. The library
provides JWT payload decoding and time-based expiry reporting, cache push
invocation, and administrator token-issuance invocation.

JWT decoding reads payload data; it does **not** verify token signatures or
authorize access. Expiry checks take an explicit clock value, so consumers can
test their policy deterministically. Push and issuance operations invoke the
caller-supplied Attic executable rather than another product frontend.

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

See [API documentation](https://docs.rs/harbor-cache) for operation options and
the [Harbor repository](https://github.com/caniko/harbor) for Nix integration.
This crate does not require Canix fleet inventory or credentials.

Licensed under MIT OR Apache-2.0; both license texts are included in the crate.
