import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  parseClientMessage,
  ProtocolParseError,
  serializeServerMessage,
  type ServerMessage,
} from "./protocol.js";

type ProtocolFixture = {
  name: string;
  direction: "client" | "server";
  message: Record<string, unknown>;
};

const fixtureUrl = new URL(
  "../../tests/fixtures/protocol/messages.json",
  import.meta.url,
);
const fixtures = JSON.parse(
  readFileSync(fixtureUrl, "utf8"),
) as ProtocolFixture[];

for (const fixture of fixtures) {
  test(`round-trips ${fixture.name}`, () => {
    if (fixture.direction === "client") {
      assert.deepEqual(
        parseClientMessage(JSON.stringify(fixture.message)),
        fixture.message,
      );
    } else {
      assert.deepEqual(
        JSON.parse(serializeServerMessage(fixture.message as ServerMessage)),
        fixture.message,
      );
    }
  });
}

test("rejects malformed JSON", () => {
  assert.throws(() => parseClientMessage("{"), ProtocolParseError);
});

test("rejects a non-object options value", () => {
  assert.throws(
    () =>
      parseClientMessage(
        JSON.stringify({
          type: "format",
          id: 1,
          fileName: "/example.ts",
          sourceText: "",
          options: [],
        }),
      ),
    /options must be an object/,
  );
});

test("rejects unsafe request identifiers", () => {
  assert.throws(
    () =>
      parseClientMessage(
        JSON.stringify({
          type: "format",
          id: Number.MAX_SAFE_INTEGER + 1,
          fileName: "/example.ts",
          sourceText: "",
          options: {},
        }),
      ),
    /id must be a non-negative integer/,
  );
});
