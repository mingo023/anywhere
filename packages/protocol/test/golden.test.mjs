import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";
import { Either, Schema } from "effect";
import { ClientMessage, ServerMessage } from "../dist/index.js";

// pocketd writes these fixtures; excess properties fail so a field added in Go
// without a matching schema change is caught here.
const golden = new URL("../../pocketd/internal/proto/testdata/golden/", import.meta.url);
const schemas = { client: ClientMessage, server: ServerMessage };

for (const [side, schema] of Object.entries(schemas)) {
  const dir = new URL(`${side}/`, golden);
  for (const name of readdirSync(dir)) {
    test(`${side}/${name}`, () => {
      const raw = JSON.parse(readFileSync(new URL(name, dir), "utf8"));
      const decoded = Schema.decodeUnknownEither(schema)(raw, { onExcessProperty: "error" });
      assert.ok(Either.isRight(decoded), Either.isLeft(decoded) ? String(decoded.left) : "");
    });
  }
}
