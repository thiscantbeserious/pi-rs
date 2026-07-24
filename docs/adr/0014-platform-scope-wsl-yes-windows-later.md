# V1 platforms: Linux, macOS, WSL. Native Windows post-parity

The v1 parity bar (ADR 0007) carries an explicit platform carve-out: Linux, macOS, and WSL are supported and tested (Linux/macOS CI lanes plus a WSL smoke job). Native Windows - named-pipe transport (ADR 0006), conhost/Windows Terminal rendering quirks - is deferred to post-parity and documented as unsupported until then. An honest "not yet" beats shipping broken (pitfall P7: Codex fine on macOS, broken on Windows Terminal/WSL [[1]](https://github.com/openai/codex/discussions/1174)). The named-pipe path in Deno also carries an active regression [[2]](https://github.com/denoland/deno/issues/33366).

## Considered Options

- Native Windows in v1 - rejected: adds a platform the author does not daily-drive to the critical path, with notoriously time-expensive terminal quirks
- Linux + macOS only - rejected: WSL coverage is nearly free (it is the Linux build) and a hard "no Windows" shrinks the future audience for no savings

## Consequences

- README must state the support matrix explicitly
- The Host Protocol keeps its transport abstraction honest so named pipes slot in later without protocol changes
- A windows-latest CI lane and Windows Terminal testing are part of the post-parity plan, not an afterthought

## Sources

1. Codex CLI scrollback and rendering broken on Windows Terminal/WSL while macOS works: https://github.com/openai/codex/discussions/1174
2. Deno named-pipe regression, connection events stop after first client disconnect: https://github.com/denoland/deno/issues/33366
