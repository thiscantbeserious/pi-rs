# Subagents stay an extension. The Core is designed subagent-aware

Subagent orchestration is not embedded in the Core - it remains extension-provided, as in pi. But the Core is designed with subagent workloads as a first-class consideration: a headless run mode (no TUI, machine-consumable output) suitable as a child process, clean child-process lifecycle primitives (spawn, cancellation, no zombies - GOALS.md goal 2), and session handling that tolerates concurrent child sessions. During the transition, the unmodified pi-subagents extension spawns pi (Node) children - acceptable while pi is installed alongside for dogfooding anyway (ADR 0008). Post-parity, headless pi-rs becomes the drop-in child.

## Considered Options

- Core-native embedded subagent runtime - rejected: re-implements the most complex extension instead of running it unmodified, violating the extension-compat promise
- Ignore subagents until post-parity - rejected: retrofitting a headless mode and child-lifecycle discipline into a TUI-shaped Core is exactly the kind of foundational rework the dogfood checkpoint exists to prevent

## Consequences

- Headless mode is a Core design constraint from day one, even though the subagent feature lands later
- The pi-subagents extension is a mandatory member of the compat spike corpus (ADR 0002) and the parity test suite
