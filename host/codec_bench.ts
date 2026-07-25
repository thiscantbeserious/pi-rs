// Host-side codec benchmark (ADR 0006, pitfall P17). Measures @msgpack/msgpack
// vs msgpackr on the actual Phase 1 protocol payload mix: small control
// messages AND large binary payloads (tool output, extension UI frame buffers
// with ANSI/UTF-8 blobs). Text-only or binary-only would be unfair in opposite
// directions per P17.
//
// Run: deno run --allow-net --allow-env --minimum-dependency-age=0 host/codec_bench.ts
//
// Decision threshold (docs/plans/step-4-host-codec-benchmark.md): the ADR 0006
// default (@msgpack/msgpack) must not be more than 2x slower than msgpackr on
// the geometric mean across payloads (encode + decode). Above 2x fires the
// armed codec-swap trigger.

import { msgpackCodec, msgpackrCodec, type Codec } from "./codec.ts";

const codecs: Codec[] = [msgpackCodec, msgpackrCodec];

// A large binary payload: ~1 MiB of ANSI-laden tool output, the kind ADR 0006
// says msgpack wins on (binary-safety, no JSON escaping).
function largeBinaryPayload(size: number): Uint8Array {
	const bytes = new Uint8Array(size);
	for (let i = 0; i < size; i++) {
		// Mix of printable ASCII and ANSI escape bytes, simulating tool output.
		bytes[i] = i % 128;
	}
	return bytes;
}

const payloads: { name: string; value: unknown; encodeBytes: number }[] = [
	{
		name: "Handshake",
		value: { type: "Handshake", protocol_version: 1, host_pid: 1234 },
		encodeBytes: 0,
	},
	{ name: "Heartbeat", value: { type: "Heartbeat" }, encodeBytes: 0 },
	{
		name: "Shutdown",
		value: { type: "Shutdown", drain: true },
		encodeBytes: 0,
	},
	{
		name: "EchoRequest-small",
		value: {
			type: "EchoRequest",
			request_id: 42,
			payload: new Uint8Array([1, 2, 3, 4, 5]),
		},
		encodeBytes: 0,
	},
	{
		name: "ProtocolError",
		value: {
			type: "ProtocolError",
			code: "MalformedFrame",
			message: "bad frame",
		},
		encodeBytes: 0,
	},
	{
		name: "EchoRequest-1MiB-binary",
		value: {
			type: "EchoRequest",
			request_id: 99,
			payload: largeBinaryPayload(1024 * 1024),
		},
		encodeBytes: 0,
	},
];

// Warmup: let V8 JIT optimize both paths before measuring.
const WARMUP = 200;
const ITERATIONS = 2000;
const LARGE_ITERATIONS = 50; // 1 MiB payloads are slower; fewer iterations

function opsPerSecond(totalMs: number, iterations: number): number {
	return (iterations / totalMs) * 1000;
}

function benchEncode(codec: Codec, value: unknown, iterations: number): number {
	for (let i = 0; i < WARMUP; i++) codec.encode(value);
	const start = performance.now();
	for (let i = 0; i < iterations; i++) codec.encode(value);
	return performance.now() - start;
}

function benchDecode(codec: Codec, bytes: Uint8Array, iterations: number): number {
	for (let i = 0; i < WARMUP; i++) codec.decode(bytes);
	const start = performance.now();
	for (let i = 0; i < iterations; i++) codec.decode(bytes);
	return performance.now() - start;
}

// Geometric mean (fair across orders-of-magnitude differences).
function geomean(values: number[]): number {
	return values.reduce((a, b) => a * b, 1) ** (1 / values.length);
}

console.log(
	`codec benchmark: ${WARMUP} warmup, encode/decode iterations vary by payload size`,
);
console.log(`payloads: ${payloads.map((p) => p.name).join(", ")}\n`);

const ratios: number[] = [];

for (const { name, value } of payloads) {
	const isLarge = name.includes("1MiB");
	const iters = isLarge ? LARGE_ITERATIONS : ITERATIONS;
	console.log(`--- ${name} (${iters} iterations) ---`);

	const results: Record<
		string,
		{
			encMs: number;
			decMs: number;
			encOps: number;
			decOps: number;
			bytes: number;
		}
	> = {};
	let encodedBytes = 0;

	for (const codec of codecs) {
		const encMs = benchEncode(codec, value, iters);
		const encoded = codec.encode(value);
		encodedBytes = encoded.byteLength;
		const decMs = benchDecode(codec, encoded, iters);
		const encOps = opsPerSecond(encMs, iters);
		const decOps = opsPerSecond(decMs, iters);
		results[codec.name] = {
			encMs,
			decMs,
			encOps,
			decOps,
			bytes: encoded.byteLength,
		};
		console.log(
			`  ${codec.name.padEnd(20)} encode ${encOps.toFixed(0).padStart(8)} ops/s | decode ${decOps.toFixed(0).padStart(8)} ops/s | ${encoded.byteLength} bytes`,
		);
	}

	// Ratio: msgpack time / msgpackr time. >1 means msgpack is slower. >2 fires the trigger.
	const msgpack = results[msgpackCodec.name];
	const msgpackr = results[msgpackrCodec.name];
	const encRatio = msgpack.encMs / msgpackr.encMs;
	const decRatio = msgpack.decMs / msgpackr.decMs;
	const combinedRatio =
		(msgpack.encMs + msgpack.decMs) / (msgpackr.encMs + msgpackr.decMs);
	ratios.push(combinedRatio);
	console.log(
		`  ratio (@msgpack/msgpack / msgpackr): encode ${encRatio.toFixed(2)}x | decode ${decRatio.toFixed(2)}x | combined ${combinedRatio.toFixed(2)}x`,
	);
	console.log(`  encoded size: ${encodedBytes} bytes\n`);
}

const geomeanRatio = geomean(ratios);
console.log(
	`=== geomean combined ratio (@msgpack/msgpack / msgpackr): ${geomeanRatio.toFixed(2)}x ===`,
);
console.log(`trigger threshold: 2.00x (above fires the codec swap)`);
if (geomeanRatio > 2) {
	console.log(
		`RESULT: @msgpack/msgpack LOSES by more than 2x. Fire the codec-swap trigger.`,
	);
} else {
	console.log(
		`RESULT: @msgpack/msgpack holds (within 2x of msgpackr). Keep ADR 0006 default.`,
	);
}
