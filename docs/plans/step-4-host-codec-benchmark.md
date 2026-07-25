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

This is a benchmark, not a correctness feature, so "RED" is "no benchmark exists, no numbers recorded." The benchmark script is the test. The codec interface gets a round-trip correctness test (encode then decode equals input) for each codec.

1. Codec interface + a round-trip test per codec (RED: no interface. GREEN: interface + both impls).
2. Benchmark script measuring both codecs on the payload mix. Output a table.
3. Run the benchmark, record the numbers + decision in docs/research.md.

## Oracle citations

The codec choice is pi-rs-native (pi is single-process; the Host Protocol is new). No pi equivalent to cite per workflow step 9. ADR 0006 is the design source.

## Result (recorded after the benchmark ran)

Geomean combined encode+decode ratio (@msgpack/msgpack / msgpackr): **1.69x**, within the 2x threshold. ADR 0006's default (`@msgpack/msgpack`) holds. The codec-swap trigger did not fire.

| Payload | Combined ratio | Notes |
| --- | --- | --- |
| Handshake | ~1.5x | small control message |
| Heartbeat | ~1.5x | small control message |
| Shutdown | ~1.5x | small control message |
| EchoRequest-small | ~1.3x | small with binary payload |
| ProtocolError | ~1.7x | small with string message |
| EchoRequest-1MiB-binary | 0.83x | msgpackr faster on large binary decode (2.86x), @msgpack/msgpack faster on encode |

Notable: the 1 MiB binary decode case is where msgpackr's native acceleration shows (2.86x faster than @msgpack/msgpack). On small messages msgpackr wins by ~1.3-1.7x. The decision holds but the margin (1.69x) is close enough that a future re-benchmark is warranted if the protocol payload mix shifts toward large binary frames.

## Out of scope

- Wiring the chosen codec into the actual host transport (step 5).
- The Deno conformance test reader (step 6 uses whatever codec step 4 picks).
- JSON as a contender (ADR 0006 rejected JSON-over-stdio; P17 already settled text decode).
