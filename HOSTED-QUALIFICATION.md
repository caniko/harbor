# Hosted Harbor-DB source qualification

This successor starts from merged trunk
`38ebf2cfbca678cc1ab79a96d6968002fb382b5e`. It does not inherit the acceptance
of the earlier `7a4c4b9ff4eb3b91c0e3856e8d14ce82d4262fd1` source.

The PR workflow checks the exact head with all sixteen original installables,
the same assertions and deadlines, and bounded concurrency. The lifecycle test
output retains a machine-readable summary. It requires at least the accepted
175 cases and zero failures, errors, skips, expected failures, or retries.

Native GitHub artifact retention is now 31 days in both the producer policy and
its generated companion. The qualification workflow independently reads actual
provider timestamps and requires a lifetime of at least 2,592,000 seconds for
every required artifact, including the retention audit. Source, workflow,
lockfiles, build outputs, reports, and raw logs are SHA-256 bound.

The high-memory gate requires a supported organization-owned larger runner.
`QUALIFICATION_LARGER_RUNNER` names it; `HOSTED_RUNNER_READ_TOKEN` supplies
organization runner-read access. Provider readiness and repository access must
pass before scheduling. The personal-account fork currently lacks this owner
prerequisite. Ordinary native evidence cannot replace high-memory qualification.

All execution is hosted-only. Failed evidence stays inspectable, and repaired
source must obtain attempt-1 GREEN on its exact successor. Composition selection,
supersession settlement, recovery execution, backups, and deployment need their
own authority and acceptance.
