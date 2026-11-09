import assert from "node:assert/strict";
import { test } from "node:test";

import { FixtureFactory, ProviderError, SchemaError } from "../src/index.js";

for (const name of ["SchemaError", "GenerationError", "ProviderError"]) {
  await test(`minimization preserves caller exception identity for ${name}`, () => {
    const sentinel = new Error("caller-owned sentinel");
    sentinel.name = name;
    assert.throws(
      () =>
        FixtureFactory.fromSchema(true).minimize("failure", {
          isFailure: () => {
            throw sentinel;
          },
        }),
      (error: unknown) => error === sentinel,
    );
  });
}

await test("provider serialization rejects accessors without invoking payload code", () => {
  let reads = 0;
  const factory = FixtureFactory.fromSchema(
    { type: "object", "x-fixture": { strategy: { kind: "provider", name: "accessor" } } },
    {
      providers: {
        accessor: () => ({
          get secret(): string {
            reads += 1;
            return "must not be read";
          },
        }),
      },
    },
  );
  assert.throws(() => factory.generate(), ProviderError);
  assert.equal(reads, 0);
});

await test("contract strings and keys reject unpaired surrogates without replacement", () => {
  assert.throws(() => FixtureFactory.fromSchema({ const: "\uD800" }), SchemaError);
  assert.throws(
    () =>
      FixtureFactory.fromSchema({
        type: "object",
        properties: { "\uD800": { const: 1 }, "\uFFFD": { const: 2 } },
      }),
    SchemaError,
  );
});

for (const seed of [Number.NaN, Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY]) {
  await test(`nonfinite seed ${String(seed)} cannot become unseeded generation`, () => {
    assert.throws(
      () =>
        FixtureFactory.fromSchema({ type: "integer", minimum: 1, maximum: 50 }).generate({ seed }),
      RangeError,
    );
  });
}

await test("custom built-in overrides can generate objects in an unlisted locale", () => {
  const factory = FixtureFactory.fromSchema(
    {
      type: "object",
      required: ["kind", "locale"],
      properties: { kind: { const: "custom" }, locale: { const: "sv-SE" } },
      "x-fixture": { provider: "email", locale: "sv-SE" },
    },
    {
      providers: { email: (context) => ({ kind: "custom", locale: context.locale }) },
    },
  );
  assert.deepEqual(factory.generate({ count: 3, seed: 42n, profile: "realistic" }), [
    { kind: "custom", locale: "sv-SE" },
    { kind: "custom", locale: "sv-SE" },
    { kind: "custom", locale: "sv-SE" },
  ]);
});
