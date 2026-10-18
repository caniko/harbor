# Changelog

## [Unreleased]

- Add shared execution protocol bindings, bounded transport and authenticated
  Rust/JavaScript channels while preserving the predecessor v1 schema bytes.
- Add private Unix controller accounting with durable one-shot reservations and
  content-bound retained output. Reopening never authorizes fresh execution.
- Add an integrity-pinned OpenCode V2 adapter for admission, one-shot initial
  release and observational reconciliation. Construction defaults to observation;
  fresh release requires the distinct journal reservation and exclusive session
  writer custody.
- Add isolated native provider/tool recovery fixtures using an existing inbox
  item's steering endpoint, including a competing-input scope control. These
  establish bounded standalone feasibility;
  production custody, source attestation and consumer acceptance remain pending.
