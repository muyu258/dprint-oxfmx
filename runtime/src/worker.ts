import { realpathSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { createInterface } from "node:readline";
import type { Readable, Writable } from "node:stream";
import { once } from "node:events";

import { format, type FormatConfig } from "oxfmt";

import {
  parseClientMessage,
  ProtocolParseError,
  serializeServerMessage,
  WORKER_PROTOCOL_VERSION,
  type ClientMessage,
  type ServerMessage,
  type WorkerError,
} from "./protocol.js";

const require = createRequire(import.meta.url);
const workerPackage = require("../package.json") as { version: string };
const oxfmtPackage = require("oxfmt/package.json") as { version: string };

export const WORKER_VERSION = workerPackage.version;
export const OXFMT_VERSION = oxfmtPackage.version;

export async function runWorker(
  input: Readable = process.stdin,
  output: Writable = process.stdout,
): Promise<void> {
  assertSupportedNodeVersion();
  const lines = createInterface({ input, crlfDelay: Number.POSITIVE_INFINITY });
  let initialized = false;

  for await (const line of lines) {
    let message: ClientMessage;
    try {
      message = parseClientMessage(line);
    } catch (error) {
      await writeMessage(output, {
        type: "error",
        error: normalizeError(error, "protocol"),
      });
      continue;
    }

    if (message.type === "shutdown") {
      await writeMessage(output, { type: "shutdownComplete" });
      lines.close();
      break;
    }

    if (message.type === "initialize") {
      if (initialized) {
        await writeMessage(output, {
          type: "error",
          error: {
            kind: "protocol",
            message: "worker is already initialized",
          },
        });
        continue;
      }

      if (message.protocolVersion !== WORKER_PROTOCOL_VERSION) {
        await writeMessage(output, {
          type: "error",
          error: {
            kind: "protocol",
            message: `worker protocol version mismatch: expected ${WORKER_PROTOCOL_VERSION}, received ${message.protocolVersion}`,
          },
        });
        continue;
      }

      initialized = true;
      await writeMessage(output, {
        type: "initialized",
        protocolVersion: WORKER_PROTOCOL_VERSION,
        workerVersion: WORKER_VERSION,
        oxfmtVersion: OXFMT_VERSION,
      });
      continue;
    }

    if (!initialized) {
      await writeMessage(output, {
        type: "error",
        id: message.id,
        error: {
          kind: "protocol",
          message: "worker must be initialized before formatting",
          fileName: message.fileName,
        },
      });
      continue;
    }

    await formatRequest(message, output);
  }
}

async function formatRequest(
  message: Extract<ClientMessage, { type: "format" }>,
  output: Writable,
): Promise<void> {
  try {
    const result = await format(
      message.fileName,
      message.sourceText,
      message.options as FormatConfig,
    );
    await writeMessage(output, {
      type: "formatResult",
      id: message.id,
      code: result.code,
      errors: result.errors.map((error) => ({
        severity: error.severity,
        message: error.message,
        labels: error.labels.map((label) => ({ ...label })),
        helpMessage: error.helpMessage,
        codeframe: error.codeframe,
      })),
    });
  } catch (error) {
    await writeMessage(output, {
      type: "error",
      id: message.id,
      error: normalizeError(error, "format", message.fileName),
    });
  }
}

async function writeMessage(
  output: Writable,
  message: ServerMessage,
): Promise<void> {
  if (!output.write(`${serializeServerMessage(message)}\n`)) {
    await once(output, "drain");
  }
}

function normalizeError(
  error: unknown,
  kind: WorkerError["kind"],
  fileName?: string,
): WorkerError {
  if (error instanceof ProtocolParseError) {
    return { kind: "protocol", message: error.message, stack: error.stack };
  }
  if (error instanceof Error) {
    return { kind, message: error.message, stack: error.stack, fileName };
  }
  return { kind, message: String(error), fileName };
}

export function assertSupportedNodeVersion(): void {
  const expected = "24.16.0";
  if (process.versions.node !== expected) {
    throw new Error(
      `Oxfmt worker requires Node ${expected}; detected ${process.versions.node}`,
    );
  }
}

const entryPath = process.argv[1];
if (
  entryPath !== undefined &&
  realpathSync(fileURLToPath(import.meta.url)) === realpathSync(entryPath)
) {
  runWorker().catch(async (error: unknown) => {
    const message: ServerMessage = {
      type: "error",
      error: normalizeError(error, "internal"),
    };
    process.stdout.write(`${serializeServerMessage(message)}\n`);
    process.exitCode = 1;
  });
}
