// Round-trip correctness test for each codec (RED-GREEN for the codec
// interface). A codec that cannot round-trip the protocol payload shapes is
// disqualified before the benchmark even runs.
//
// Also tests cross-codec decode: every encoder's output must decode with every
// codec. A self-round-trip can pass when a codec emits a representation its peer
// cannot consume; the cross-codec check catches that.
//
// Run: deno test --allow-net --allow-env --minimum-dependency-age=0 host/codec_test.ts

import { assertEquals } from "jsr:@std/assert@1";
import { msgpackCodec, msgpackrCodec, type Codec } from "./codec.ts";

const codecs: Codec[] = [msgpackCodec, msgpackrCodec];

// Normalize binary payloads for comparison: msgpackr decodes binary as Node
// Buffer, @msgpack/msgpack as Uint8Array. Both are valid msgpack; the bytes
// must match even when the constructor differs.
function normalize(value: unknown): unknown {
	if (
		value instanceof Uint8Array ||
		(typeof Buffer !== "undefined" && value instanceof Buffer)
	) {
		return Array.from(new Uint8Array(value as Uint8Array));
	}
	if (value && typeof value === "object") {
		if (Array.isArray(value)) return value.map(normalize);
		const out: Record<string, unknown> = {};
		for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
			out[k] = normalize(v);
		}
		return out;
	}
	return value;
}

// The protocol payload shapes from ADR 0022's message set. Mirrors the Rust
// fixtures: small control messages and a struct with nested fields.
const payloads: { name: string; value: unknown }[] = [
	{
		name: "Handshake",
		value: { type: "Handshake", protocol_version: 1, host_pid: 1234 },
	},
	{ name: "Heartbeat", value: { type: "Heartbeat" } },
	{ name: "Shutdown", value: { type: "Shutdown", drain: true } },
	{
		name: "EchoRequest",
		value: {
			type: "EchoRequest",
			request_id: 42,
			payload: new Uint8Array([1, 2, 3, 4, 5]),
		},
	},
	{
		name: "ProtocolError",
		value: {
			type: "ProtocolError",
			code: "MalformedFrame",
			message: "bad frame",
		},
	},
];

Deno.test("each codec round-trips every protocol payload shape", () => {
	for (const codec of codecs) {
		for (const { name, value } of payloads) {
			const encoded = codec.encode(value);
			const decoded = codec.decode(encoded);
			assertEquals(
				normalize(decoded),
				normalize(value),
				`${codec.name} round-trip failed for ${name}`,
			);
		}
	}
});

Deno.test("cross-codec decode: every encoder's output decodes with every codec", () => {
	// A self-round-trip can pass when a codec emits a representation its peer
	// cannot consume. Decode every encoder's bytes with every codec to catch
	// representation incompatibility.
	for (const { name, value } of payloads) {
		for (const encoder of codecs) {
			const bytes = encoder.encode(value);
			for (const decoder of codecs) {
				const decoded = decoder.decode(bytes);
				assertEquals(
					normalize(decoded),
					normalize(value),
					`${decoder.name} could not decode ${encoder.name}'s ${name}`,
				);
			}
		}
	}
});

Deno.test("each codec handles the max-safe-integer request_id boundary", () => {
	// ADR 0022 Q6 + the request_id 53-bit constraint documented in messages.rs.
	const value = {
		type: "EchoRequest",
		request_id: 9_007_199_254_740_991, // 2^53 - 1
		payload: new Uint8Array([0]),
	};
	for (const codec of codecs) {
		const encoded = codec.encode(value);
		const decoded = codec.decode(encoded) as typeof value;
		assertEquals(
			decoded.request_id,
			value.request_id,
			`${codec.name} lost precision on the 2^53-1 boundary`,
		);
	}
});
