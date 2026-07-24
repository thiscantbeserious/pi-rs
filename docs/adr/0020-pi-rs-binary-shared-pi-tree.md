# pi-rs binary name, fully shared ~/.pi config tree

During coexistence the binary is named pi-rs (a pi alias ships only post-parity, if and when it replaces pi). Configuration is pi's own tree, shared completely: extensions, themes, models.json, auth.json, settings.json, and sessions all live in ~/.pi/agent/ and are read by both tools. pi-rs writes only formats pi can read, extending ADR 0008's session discipline to the entire tree. Dogfooding needs zero migration and the setup has one source of truth.

## Considered Options

- Own ~/.pi-rs tree with an importer - rejected: every setting change during dogfood happens twice, and sessions/auth interop (ADRs 0008/0019) would need cross-tree special cases
- Binary named pi with PATH ordering - rejected: maximum confusion in the exact phase where both tools run daily

## Consequences

- settings.json compatibility becomes part of the parity surface: pi-rs must tolerate unknown keys and never rewrite the file destructively
- A pi-rs config bug can hurt pi too. Mitigation: sole-writer discipline, tolerant readers, CI gates
- auth.json tokens are read and refreshed by both tools (ADR 0019), refresh races must be handled (last-writer-wins with atomic file replace)
