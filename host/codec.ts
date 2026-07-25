// Host-side codec interface (ADR 0006, pitfall P17). The codec is swappable
// behind this interface so the armed trigger (codec swap on benchmark failure)
// is a one-line change, not a protocol change.
//
// Phase 1 step 4 benchmarks the two contenders (@msgpack/msgpack vs msgpackr)
// and records the decision in docs/research.md. The default per ADR 0006 is
// @msgpack/msgpack; the benchmark decides whether that holds.

import { encode as encodeMsgpack, decode as decodeMsgpack } from "npm:@msgpack/msgpack@3.1.3";
import { pack as packMsgpackr, unpack as unpackMsgpackr } from "npm:msgpackr@2.0.4";

/** A Host Protocol codec: encode a Message to bytes, decode bytes back. */
export interface Codec {
  readonly name: string;
  encode(value: unknown): Promise<Uint8Array>;
  decode(bytes: Uint8Array): Promise<unknown>;
}

/** The @msgpack/msgpack reference implementation (ADR 0006 default). */
export const msgpackCodec: Codec = {
  name: "@msgpack/msgpack",
  async encode(value: unknown): Promise<Uint8Array> {
    return encodeMsgpack(value);
  },
  async decode(bytes: Uint8Array): Promise<unknown> {
    return decodeMsgpack(bytes);
  },
};

/** The msgpackr contender (P17 benchmark). */
export const msgpackrCodec: Codec = {
  name: "msgpackr",
  async encode(value: unknown): Promise<Uint8Array> {
    return packMsgpackr(value);
  },
  async decode(bytes: Uint8Array): Promise<unknown> {
    return unpackMsgpackr(bytes);
  },
};
