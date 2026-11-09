import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { z } from "zod";

import {
  FixtureFactory,
  GenerationError,
  ProviderError,
  SchemaError,
  fixture,
  stripFixtureMetadata,
} from "../src/index.js";
import type { JSONValue, Schema } from "../src/index.js";

await test("sidecar operation selection resolves refs and preserves typed views and replay", () => {
  const source = fileURLToPath(new URL("../../../examples/vendor.yaml", import.meta.url));
  const sidecar = fileURLToPath(new URL("../../../examples/vendor.fixtures.yaml", import.meta.url));
  const factory = FixtureFactory.fromFile(source, {
    path: "/customers/{id}",
    fixtureFile: sidecar,
  });
  const first = factory.generate({ count: 30, seed: 42n, profile: "realistic" });
  assert.deepEqual(first, factory.generate({ count: 30, seed: 42n, profile: "realistic" }));
  const customer = z.object({
    id: z.string(),
    name: z.string(),
    status: z.string(),
    balance_cents: z.number().int(),
  });
  const cases = factory
    .asModel(customer)
    .successCases({ count: 30, seed: 42n, profile: "realistic" });
  assert.ok(cases.some((value) => value.value.name === "李明"));
  assert.ok(cases.some((value) => value.value.name.includes(" ")));
  assert.ok(
    factory
      .failureCases({ count: 10, seed: 42n })
      .every((value) => value.target === "/name" && !value.schema_valid),
  );
});

await test("typed views retain allowed extras in payload and reject dropped required fields", () => {
  const source = { name: "Ada", vendor_debug: { trace: 7 } };
  const model = z.object({ name: z.string() });
  const cases = FixtureFactory.fromSchema({
    type: "object",
    required: ["name"],
    properties: { name: { const: "Ada" } },
    "x-fixture": { strategy: { kind: "values", values: [source] } },
  })
    .asModel(model)
    .successCases();
  const [first] = cases;
  assert.ok(first);
  assert.equal(first.value.name, "Ada");
  assert.deepEqual(first.payload, source);
  assert.throws(
    () => FixtureFactory.fromSchema({ const: source }).asModel(model).generate(),
    GenerationError,
  );
});

await test("composed strategies filter candidates and explicit negative strategies violate bounds", () => {
  const factory = FixtureFactory.fromSchema({
    type: "integer",
    minimum: 10,
    maximum: 12,
    ...fixture({
      strategy: {
        kind: "filter",
        schema: { minimum: 10 },
        strategy: {
          kind: "one-of",
          strategies: [
            { kind: "values", values: [-1, 10] },
            { kind: "values", values: [11, 12] },
          ],
        },
      },
      fail: { strategy: { kind: "schema", constraints: "violate" } },
    }),
  });
  assert.deepEqual(new Set(factory.generate({ count: 80, seed: 42 })), new Set([10, 11, 12]));
  assert.deepEqual(
    new Set(factory.failureCases({ count: 30, seed: 42 }).map((value) => value.value)),
    new Set([9, 13]),
  );
  assert.throws(
    () =>
      FixtureFactory.fromSchema({
        type: "integer",
        ...fixture({
          strategy: {
            kind: "filter",
            schema: { minimum: 10 },
            strategy: { kind: "values", values: [0] },
          },
        }),
      }).generate({ seed: 42, maxAttempts: 2 }),
    GenerationError,
  );
});

await test("custom provider contexts retain 64-bit seeds and isolate each invocation", () => {
  const seeds: bigint[] = [];
  const factory = FixtureFactory.fromSchema(
    {
      type: "string",
      "x-fixture": { provider: "synthetic.id", options: { nested: { value: "original" } } },
    },
    {
      providers: {
        "synthetic.id": (context) => {
          seeds.push(context.seed);
          const { nested } = context.options;
          if (nested === null || typeof nested !== "object" || Array.isArray(nested)) {
            throw new Error("Expected fixture options.");
          }
          const { value: prior } = nested;
          if (typeof prior !== "string") {
            throw new TypeError("Expected a string option.");
          }
          nested["value"] = "changed";
          return `${context.seed.toString(16)}:${prior}`;
        },
      },
    },
  );
  const first = factory.generate({ count: 5, seed: 42n, profile: "realistic" });
  assert.ok(seeds.some((seed) => seed > BigInt(Number.MAX_SAFE_INTEGER) && seed % 2048n !== 0n));
  assert.ok(first.every((value) => typeof value === "string" && value.endsWith(":original")));
  assert.deepEqual(first, factory.generate({ count: 5, seed: 42n, profile: "realistic" }));
  assert.equal(new Set(first).size, 5);
});

await test("non-JSON providers and unsupported metadata fail through public errors", () => {
  assert.throws(
    () => FixtureFactory.fromSchema({ type: "string", "x-fixture": { provider: "missing" } }),
    SchemaError,
  );
  assert.throws(
    () =>
      FixtureFactory.fromSchema(
        { "x-fixture": { provider: "broken" } },
        { providers: { broken: () => Number.NaN } },
      ).generate({ profile: "realistic" }),
    ProviderError,
  );
  assert.throws(
    () =>
      FixtureFactory.fromSchema(
        { type: "integer" },
        { fixtures: { version: "0.1", fields: { "": { fail: { examples: [Number.NaN] } } } } },
      ),
    SchemaError,
  );
  assert.throws(
    () =>
      FixtureFactory.fromSchema({ type: "integer" }).generate({
        seed: Number.MAX_SAFE_INTEGER + 1,
      }),
    z.ZodError,
  );
});

await test("native numeric projection rejects values that JavaScript cannot preserve", () => {
  const factory = FixtureFactory.fromSchema({
    type: "number",
    minimum: 9,
    maximum: 9.1,
    multipleOf: 0.1234567890123456,
  });
  assert.throws(() => factory.generate({ count: 2, seed: 42 }), GenerationError);
});

await test("file readers reject rounded integer and decimal constraints", () => {
  const directory = mkdtempSync(path.join(os.tmpdir(), "riff-numbers-"));
  try {
    const source = path.join(directory, "schema.yaml");
    writeFileSync(source, "type: integer\nminimum: 9007199254740993\n");
    assert.throws(() => FixtureFactory.fromFile(source), SchemaError);
    writeFileSync(source, "type: number\nminimum: 0.123456789012345678901234567\n");
    assert.throws(() => FixtureFactory.fromFile(source), SchemaError);
  } finally {
    rmSync(directory, { recursive: true });
  }
});

await test("minimization retains failure and does not mutate its input", () => {
  const factory = FixtureFactory.fromSchema({
    type: "object",
    required: ["name"],
    properties: { name: { type: "string", minLength: 1 } },
  });
  const original: JSONValue = { name: "prefix!suffix", unrelated: [1, 2, 3] };
  const result = factory.minimize(original, {
    isFailure: (value) =>
      value !== null &&
      typeof value === "object" &&
      !Array.isArray(value) &&
      typeof value["name"] === "string" &&
      value["name"].includes("!"),
  });
  assert.deepEqual(result.value, { name: "!" });
  assert.deepEqual(original, { name: "prefix!suffix", unrelated: [1, 2, 3] });
  assert.equal(result.budgetExhausted, false);
});

await test("publication removes annotations without removing literal property or example data", () => {
  const document = {
    openapi: "3.1.0",
    components: {
      schemas: {
        Payload: {
          type: "object",
          "x-fixture": { provider: "whole" },
          properties: {
            "x-fixture": { type: "integer", minimum: 1, "x-fixture": { examples: [1] } },
          },
          examples: [{ "x-fixture": "literal" }],
        },
      },
    },
  };
  assert.deepEqual(stripFixtureMetadata(document), {
    openapi: "3.1.0",
    components: {
      schemas: {
        Payload: {
          type: "object",
          properties: { "x-fixture": { type: "integer", minimum: 1 } },
          examples: [{ "x-fixture": "literal" }],
        },
      },
    },
  });
});

await test("CLI emits replayable JSONL and emits no partial output on failure", () => {
  const cli = fileURLToPath(new URL("../src/cli.js", import.meta.url));
  const source = fileURLToPath(new URL("../../../examples/endpoint-vendor.yaml", import.meta.url));
  const args = [
    cli,
    source,
    "--path",
    "/customers/{id}",
    "--count",
    "5",
    "--seed",
    "42",
    "--expectation",
    "fail",
    "--jsonl",
  ];
  const first = spawnSync(process.execPath, args, { encoding: "utf8" });
  const replay = spawnSync(process.execPath, args, { encoding: "utf8" });
  assert.equal(first.status, 0, first.stderr);
  assert.equal(first.stdout, replay.stdout);
  const cases = first.stdout
    .trim()
    .split("\n")
    .map((line) => {
      const input: unknown = JSON.parse(line);
      return z.object({ expectation: z.literal("fail"), target: z.string() }).parse(input);
    });
  assert.equal(cases.length, 5);
});

await test("CLI failure does not emit a partial batch", () => {
  const cli = fileURLToPath(new URL("../src/cli.js", import.meta.url));
  const source = fileURLToPath(new URL("../../../examples/endpoint-vendor.yaml", import.meta.url));
  const failed = spawnSync(process.execPath, [cli, source, "--count", "-1"], { encoding: "utf8" });
  assert.equal(failed.status, 2);
  assert.equal(failed.stdout, "");
});

await test("unevaluated tuple values preserve the native array contract", () => {
  const schema: Schema = {
    type: "array",
    prefixItems: [{ const: "head" }],
    minItems: 2,
    maxItems: 2,
    unevaluatedItems: { const: 17 },
  };
  assert.deepEqual(FixtureFactory.fromSchema(schema).generate({ count: 3, seed: 42 }), [
    ["head", 17],
    ["head", 17],
    ["head", 17],
  ]);
});
