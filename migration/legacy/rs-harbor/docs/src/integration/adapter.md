# mkAdapter

`mkAdapter` builds a typed `harbor-adapter` value that describes how a downstream project should talk to an Attic binary cache.

## Example

```nix
harborAdapter = rs-harbor.lib.mkAdapter {
  attic = {
    endpoint = "https://cache.example.com";
    cache = "main";
  };
};
```

The adapter includes:

- cache endpoint
- cache name
- token environment variable, defaulting to `ATTIC_TOKEN`
- a small type marker so `mkAtticPush` can validate what it receives

Use [mkAtticPush](./attic-push.md) to turn the adapter into a runnable Nix app.
