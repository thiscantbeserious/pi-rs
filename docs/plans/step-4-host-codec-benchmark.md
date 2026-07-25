# Step 4 plan: host-side codec benchmark (P17)

Pitfall P17 and ADR 0006 require benchmarking `@msgpack/msgpack` vs `msgpackr` at protocol bring-up, decided by measurements not claims. The codec stays behind an interface so the armed trigger (codec swap) is cheap. No ADR change unless the benchmark fires the trigger.

## Scope

1. A Deno benchmark script under `host/` that encodes and decodes the actual Phase 1 protocol payload mix through both codecs.
2. The payload mix reflects what ADR 0006 says crosses the wire: small control messages (Handshake, Heartbeat) and large binary payloads (tool output, extension UI frame buffers with ANSI/UTF-8 blobs). Text-only or binary-only would be unfair in opposite directions.
3. A codec interface in the host so the choice is swappable without protocol changes (P17 guard).
4. A decision recorded in `docs/research.md` with the numbers: which codec, by what margin, on which payload. The armed trigger fires only if the chosen codec loses badly (defined below).

## Research findings (verified against current sources, not training data)

- `@msgpack/msgpack` 3.1.3 (npm latest). The reference JS implementation ADR 0006 names.
- `msgpackr` 2.0.4 (npm latest). Claims "ultra-fast", supports `records` (structured cloning) and `structures` (shared key tables) for extra speed on repeated shapes.
- P17 (docs/pitfalls.md): V8's JSON.parse is heavily optimized for text; msgpack does not win on raw text decode. The msgpack win is binary-safety and payload size. The benchmark must reflect this, not re-litigate text decode speed.

## Decision threshold (the armed trigger)

"Codec benchmark failure fires the codec swap" (ROADMAP). Concrete threshold:

- The chosen codec (ADR 0006 default: `@msgpack/msgpack`) must not be more than 2x slower than the alternative on the protocol payload mix (geometric mean across payload types, encode + decode).
- 2x is the bar because below that, the binary-safety and reference-implementation benefits outweigh the speed difference; above 2x, the host's hot path (streaming frames) is measurably harmed.
- If `@msgpack/msgpack` loses by more than 2x, fire the trigger: switch the host codec to `msgpackr`, record the decision in a superseding note to ADR 0006 (not a new ADR; ADR 0006 already names the codec as swappable behind the interface).

## TDD order

This is a benchmark, not a correctness feature, so "RED" is "no benchmark exists, no numbers recorded." The benchmark script is the test. The codec interface gets correctness tests.

1. Codec interface + a round-trip test per codec (RED: no interface. GREEN: interface + both impls).
2. Cross-codec decode test: decode every encoder's output with every codec. A self-round-trip can pass when a codec emits a representation its peer cannot consume; the cross-codec check catches that (added during CodeRabbit review).
3. Benchmark script measuring both codecs on the payload mix. Output a table.
4. Run the benchmark, record the decision in `docs/research.md` (canonical) and the plan doc.

Step 5 consumes the `Codec` interface and `msgpackCodec` from `host/codec.ts` to wire the chosen codec into the host transport.

## Oracle citations

The codec choice is pi-rs-native (pi is single-process; the Host Protocol is new). No pi equivalent to cite per workflow step 9. ADR 0006 is the design source.

## Result (recorded after the benchmark ran)

**Decision: the host codec is `@msgpack/msgpack` (ADR 0006 default).** The codec-swap trigger did not fire.

Geomean combined encode+decode ratio (`@msgpack/msgpack` / `msgpackr`) measured across the protocol payload mix: consistently in the **1.7x-1.8x range across runs**, under the 2x threshold. The benchmark has run-to-run variance (JIT, GC, system load) high enough that exact per-payload numbers are not stable between runs, so they are not recorded here as fixed figures. The geomean is consistently under 2x across multiple runs, which is what the decision rests on.

Observed per-run pattern (not stable enough to be the decision basis, recorded for context):

- Small control messages (Handshake, Heartbeat, Shutdown): msgpackr faster by ~1.3x-2.7x, varies by run.
- EchoRequest-small and ProtocolError: msgpackr faster by ~1.3x-1.7x.
- EchoRequest-1MiB-binary: the most volatile case. msgpackr's native-acceleration decode path sometimes beats `@msgpack/msgpack` by ~2.9x, sometimes `@msgpack/msgpack` wins. This case alone swings the geomean.

Notable: the 1 MiB binary decode case is where msgpackr's native acceleration shows when it engages. This refines P17: the msgpack win is binary-safety and payload size, but msgpackr's native acceleration makes the large-binary-decode case volatile. The decision holds at ~1.7-1.8x geomean; re-benchmark if the protocol payload mix shifts toward large binary frames, since that case is closest to the 2x threshold and the most variable.

The decision and the per-run observations live in `docs/research.md` as the canonical record. Step 5 consumes the `Codec` interface and `msgpackCodec` from `host/codec.ts`.

## Out of scope

- Wiring the chosen codec into the actual host transport (step 5).
- The Deno conformance test reader (step 6 uses whatever codec step 4 picks).
- JSON as a contender (ADR 0006 rejected JSON-over-stdio; P17 already settled text decode).
