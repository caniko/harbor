import { decode, encode, negotiate, type Message } from "harbor-llm/execution-protocol";
import { AuthenticatedChannel } from "harbor-llm/execution-channel";
import { FrameDecoder, frame } from "harbor-llm/execution-framing";
import { runJson, type TransportLimits } from "harbor-llm/transport";
import { OpenCodeV2, type OpenCodeV2Options } from "harbor-llm/opencode-v2";

const message: Message = { schemaVersion: 1, requestId: "typed", body: { kind: "error", code: "effect-unknown" } };
const channel = new AuthenticatedChannel({ key: new Uint8Array(32), nonce: new Uint8Array(32), binding: new Uint8Array(32), role: "controller" });
const received: Message = channel.open(channel.seal(decode(encode(message))));
negotiate(received, message, ["zero-effect-reopen"]);
new FrameDecoder().push(frame(message));
const limits: TransportLimits = { timeoutMs: 10, inputBytes: 1024, outputBytes: 1024, stderrBytes: 1024 };
const result: Promise<unknown> = runJson("/bin/example", [], { limits });
void result;
// @ts-expect-error Generation counters preserve u64 precision as strings.
const invalid: Message = { schemaVersion: 2, requestId: "invalid", body: { kind: "start" } };
void invalid;

declare const options: OpenCodeV2Options;
const adapter = new OpenCodeV2({ ...options, mode: "fresh" });
void adapter.start({ model: { providerID: "synthetic", id: "fixture" }, permissions: [] });
void adapter.submit({ id: "inb_fixture", text: "fixture", delivery: "queue", resume: false });
// @ts-expect-error Recovery must never request engine resume.
void adapter.submit({ id: "inb_fixture", text: "fixture", delivery: "queue", resume: true });
void adapter.execute("inb_fixture");
// @ts-expect-error Execution release is bound to an inbox ID, not another prompt.
void adapter.execute({ id: "inb_fixture", text: "replacement" });
void adapter.reconcile();
