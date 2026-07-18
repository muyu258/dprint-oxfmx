import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { createInterface } from "node:readline";
import { PassThrough } from "node:stream";
import { fileURLToPath } from "node:url";
import test from "node:test";

import {
  WORKER_PROTOCOL_VERSION,
  type ClientMessage,
  type ServerMessage,
} from "./protocol.js";
import { OXFMT_VERSION, runWorker, WORKER_VERSION } from "./worker.js";

class WorkerHarness {
  readonly #input = new PassThrough();
  readonly #output = new PassThrough();
  readonly #lines = createInterface({ input: this.#output });
  readonly #iterator = this.#lines[Symbol.asyncIterator]();
  readonly #worker = runWorker(this.#input, this.#output);

  async request(message: ClientMessage): Promise<ServerMessage> {
    return this.requestLine(JSON.stringify(message));
  }

  async requestLine(line: string): Promise<ServerMessage> {
    this.#input.write(`${line}\n`);
    const result = await this.#iterator.next();
    assert.equal(result.done, false, "worker closed before responding");
    return JSON.parse(result.value) as ServerMessage;
  }

  async close(): Promise<void> {
    const response = await this.request({ type: "shutdown" });
    assert.deepEqual(response, { type: "shutdownComplete" });
    await this.#worker;
    this.#lines.close();
    this.#input.destroy();
    this.#output.destroy();
  }
}

test("initializes with the pinned protocol and package versions", async () => {
  const worker = new WorkerHarness();

  assert.deepEqual(
    await worker.request({
      type: "initialize",
      protocolVersion: WORKER_PROTOCOL_VERSION,
    }),
    {
      type: "initialized",
      protocolVersion: WORKER_PROTOCOL_VERSION,
      workerVersion: WORKER_VERSION,
      oxfmtVersion: OXFMT_VERSION,
    },
  );
  assert.equal(OXFMT_VERSION, "0.59.0");

  await worker.close();
});

test("formats multiple requests through one worker", async () => {
  const worker = new WorkerHarness();
  await worker.request({
    type: "initialize",
    protocolVersion: WORKER_PROTOCOL_VERSION,
  });

  assert.deepEqual(
    await worker.request({
      type: "format",
      id: 1,
      fileName: "/tmp/example.ts",
      sourceText: 'const value="hello"\n',
      options: {},
    }),
    {
      type: "formatResult",
      id: 1,
      code: 'const value = "hello";\n',
      errors: [],
    },
  );

  assert.deepEqual(
    await worker.request({
      type: "format",
      id: 2,
      fileName: "/tmp/example.ts",
      sourceText: 'const value="hello"\n',
      options: { singleQuote: true },
    }),
    {
      type: "formatResult",
      id: 2,
      code: "const value = 'hello';\n",
      errors: [],
    },
  );

  const invalidResult = await worker.request({
    type: "format",
    id: 3,
    fileName: "/tmp/example.ts",
    sourceText: "const =\n",
    options: {},
  });
  assert.equal(invalidResult.type, "formatResult");
  if (invalidResult.type === "formatResult") {
    assert.equal(invalidResult.id, 3);
    assert.equal(invalidResult.errors[0]?.severity, "Error");
    assert.equal(invalidResult.errors[0]?.message, "Unexpected token");
  }

  await worker.close();
});

test("reports protocol failures without terminating the worker", async () => {
  const worker = new WorkerHarness();

  const malformed = await worker.requestLine("{");
  assert.equal(malformed.type, "error");
  if (malformed.type === "error") {
    assert.equal(malformed.error.kind, "protocol");
  }

  const beforeInitialize = await worker.request({
    type: "format",
    id: 1,
    fileName: "/tmp/example.ts",
    sourceText: "const value=1\n",
    options: {},
  });
  assert.equal(beforeInitialize.type, "error");

  const mismatch = await worker.request({
    type: "initialize",
    protocolVersion: WORKER_PROTOCOL_VERSION + 1,
  });
  assert.equal(mismatch.type, "error");

  const initialized = await worker.request({
    type: "initialize",
    protocolVersion: WORKER_PROTOCOL_VERSION,
  });
  assert.equal(initialized.type, "initialized");

  const duplicateInitialize = await worker.request({
    type: "initialize",
    protocolVersion: WORKER_PROTOCOL_VERSION,
  });
  assert.equal(duplicateInitialize.type, "error");

  await worker.close();
});

test("runs as a standalone stdio worker", async () => {
  const child = spawn(
    process.execPath,
    [fileURLToPath(new URL("./worker.js", import.meta.url))],
    { stdio: ["pipe", "pipe", "pipe"] },
  );
  const lines = createInterface({ input: child.stdout });
  const iterator = lines[Symbol.asyncIterator]();
  let stderr = "";
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk: string) => {
    stderr += chunk;
  });

  child.stdin.write(
    `${JSON.stringify({
      type: "initialize",
      protocolVersion: WORKER_PROTOCOL_VERSION,
    })}\n`,
  );
  const initialized = await iterator.next();
  assert.equal(initialized.done, false);
  assert.equal(
    (JSON.parse(initialized.value) as ServerMessage).type,
    "initialized",
  );

  child.stdin.write(`${JSON.stringify({ type: "shutdown" })}\n`);
  const shutdown = await iterator.next();
  assert.equal(shutdown.done, false);
  assert.deepEqual(JSON.parse(shutdown.value), { type: "shutdownComplete" });

  const [exitCode, signal] = (await once(child, "exit")) as [
    number | null,
    NodeJS.Signals | null,
  ];
  assert.equal(exitCode, 0);
  assert.equal(signal, null);
  assert.equal(stderr, "");
  lines.close();
});
