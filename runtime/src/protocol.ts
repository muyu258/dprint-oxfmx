export const WORKER_PROTOCOL_VERSION = 1;
export const MAX_REQUEST_ID = 0xffff_ffff;

export type InitializeRequest = {
  type: "initialize";
  protocolVersion: number;
};

export type FormatRequest = {
  type: "format";
  id: number;
  fileName: string;
  sourceText: string;
  options: Record<string, unknown>;
};

export type ShutdownRequest = {
  type: "shutdown";
};

export type ClientMessage =
  | InitializeRequest
  | FormatRequest
  | ShutdownRequest;

export type InitializedResponse = {
  type: "initialized";
  protocolVersion: number;
  workerVersion: string;
  oxfmtVersion: string;
};

export type FormatResultResponse = {
  type: "formatResult";
  id: number;
  code: string;
};

export type WorkerErrorKind = "protocol" | "format" | "internal";

export type WorkerError = {
  kind: WorkerErrorKind;
  message: string;
  stack?: string;
  fileName?: string;
};

export type ErrorResponse = {
  type: "error";
  id?: number;
  error: WorkerError;
};

export type ShutdownCompleteResponse = {
  type: "shutdownComplete";
};

export type ServerMessage =
  | InitializedResponse
  | FormatResultResponse
  | ErrorResponse
  | ShutdownCompleteResponse;

export class ProtocolParseError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ProtocolParseError";
  }
}

export function parseClientMessage(line: string): ClientMessage {
  let value: unknown;
  try {
    value = JSON.parse(line);
  } catch (error) {
    throw new ProtocolParseError(
      `worker request is not valid JSON: ${errorMessage(error)}`,
    );
  }

  if (!isRecord(value) || typeof value.type !== "string") {
    throw new ProtocolParseError("worker request must have a string type");
  }

  switch (value.type) {
    case "initialize":
      assertNonNegativeInteger(value.protocolVersion, "protocolVersion");
      return {
        type: "initialize",
        protocolVersion: value.protocolVersion,
      };
    case "format":
      assertRequestId(value.id);
      assertString(value.fileName, "fileName");
      assertString(value.sourceText, "sourceText");
      if (!isRecord(value.options)) {
        throw new ProtocolParseError("format request options must be an object");
      }
      return {
        type: "format",
        id: value.id,
        fileName: value.fileName,
        sourceText: value.sourceText,
        options: value.options,
      };
    case "shutdown":
      return { type: "shutdown" };
    default:
      throw new ProtocolParseError(`unknown worker request type: ${value.type}`);
  }
}

export function serializeServerMessage(message: ServerMessage): string {
  return JSON.stringify(message);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function assertRequestId(value: unknown): asserts value is number {
  assertNonNegativeInteger(value, "id");
  if (value > MAX_REQUEST_ID) {
    throw new ProtocolParseError(`id must not exceed ${MAX_REQUEST_ID}`);
  }
}

function assertNonNegativeInteger(
  value: unknown,
  fieldName: string,
): asserts value is number {
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    throw new ProtocolParseError(`${fieldName} must be a non-negative integer`);
  }
}

function assertString(value: unknown, fieldName: string): asserts value is string {
  if (typeof value !== "string") {
    throw new ProtocolParseError(`${fieldName} must be a string`);
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

