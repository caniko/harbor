# Changelog

## [Unreleased]

- Add shared execution protocol bindings, bounded transport and authenticated
  Rust/JavaScript channels while preserving the predecessor v1 schema bytes.
- Add private Unix controller accounting with durable one-shot reservations and
  content-bound retained output. Reopening never authorizes fresh execution.
- Add an integrity-pinned OpenCode V2 adapter for admission and observational
  reconciliation, plus a distinct journal reservation for initial execution.
- Add isolated native provider/tool recovery fixtures using an existing inbox
  item's steering endpoint. These establish bounded standalone feasibility;
  production custody, source attestation and consumer acceptance remain pending.
