// The real Deno Extension Host entrypoint (ADR 0023 Q1).
//
// Connects to PI_RS_HOST_SOCKET, sends Handshake, replies Pong to heartbeats,
// handles Shutdown{drain:true} with ShutdownAck, echoes EchoRequest.
// In Phase 1 this is a protocol-level smoke test of the real host binary.
// Phase 3 replaces the message loop with the vendored pi runtime (ADR 0021).
//
// Compile: deno compile --allow-net --allow-env --minimum-dependency-age=0 \
//          --output host-bin host/main.ts
// Run:     PI_RS_HOST_SOCKET=/path/to.sock ./host-bin

import { msgpackCodec } from "./codec.ts";

const SOCKET_ENV = "PI_RS_HOST_SOCKET";
const PROTOCOL_VERSION = 1;

const socketPath = Deno.env.get(SOCKET_ENV);
if (!socketPath) {
  console.error(`host: ${SOCKET_ENV} not set`);
  Deno.exit(2);
}

const conn = await Deno.connect({ path: socketPath, transport: "unix" });

// Host speaks first: send Handshake (ADR 0022 Q2).
const handshake = {
  type: "Handshake",
  protocol_version: PROTOCOL_VERSION,
  host_pid: Deno.pid,
};
await writeMessage(conn, handshake);

// Wait for HandshakeAck.
const ack = await readMessage(conn);
if (ack.type !== "HandshakeAck") {
  console.error("host: expected HandshakeAck, got", ack.type);
  Deno.exit(7);
}

// Message loop: handle Heartbeat, EchoRequest, Shutdown.
while (true) {
  let msg: any;
  try {
    msg = await readMessage(conn);
  } catch {
    break; // socket closed
  }

  let reply: any = null;
  switch (msg.type) {
    case "Heartbeat":
      reply = { type: "Pong" };
      break;
    case "EchoRequest":
      reply = {
        type: "EchoResponse",
        request_id: msg.request_id,
        payload: msg.payload,
      };
      break;
    case "Shutdown":
      if (msg.drain) {
        await writeMessage(conn, { type: "ShutdownAck" });
      }
      Deno.exit(0);
      break;
  }
  if (reply) {
    await writeMessage(conn, reply);
  }
}

async function writeMessage(conn: Deno.Conn, msg: unknown): Promise<void> {
  const body = msgpackCodec.encode(msg);
  const frame = new Uint8Array(4 + body.length);
  const view = new DataView(frame.buffer);
  view.setUint32(0, body.length); // BE u32 length prefix
  frame.set(body, 4);
  await conn.write(frame);
}

async function readMessage(conn: Deno.Conn): Promise<any> {
  const lenBuf = new Uint8Array(4);
  await conn.read(lenBuf);
  const view = new DataView(lenBuf.buffer);
  const len = view.getUint32(0);
  const body = new Uint8Array(len);
  await conn.read(body);
  return msgpackCodec.decode(body);
}
