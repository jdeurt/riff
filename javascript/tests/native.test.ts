import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";

import { FixtureFactory, ProviderError, RiffError, SchemaError } from "../src/index.js";

await test("provider failures retain error causes and allow later batches", () => {
  class VendorError extends SchemaError {
    public override name = "VendorError";
  }
  const cause = new VendorError("caller-owned failure");
  assert.ok(cause instanceof VendorError);
  let unavailable = true;
  const factory = FixtureFactory.fromSchema(
    { type: "string", "x-fixture": { provider: "recovering" } },
    {
      providers: {
        recovering: () => {
          if (unavailable) {
            throw cause;
          }
          return "valid";
        },
      },
    },
  );
  assert.ok(factory instanceof FixtureFactory);
  assert.throws(
    () => factory.generate({ seed: 42n, profile: "realistic" }),
    (error: unknown) => {
      assert.ok(error instanceof ProviderError);
      assert.ok(error instanceof RiffError);
      assert.ok(error instanceof Error);
      assert.equal(error.cause, cause);
      assert.equal(error.name, "ProviderError");
      return true;
    },
  );
  unavailable = false;
  assert.deepEqual(factory.generate({ seed: 42n, profile: "realistic" }), ["valid"]);
  assert.throws(
    () =>
      factory.minimize("valid", {
        isFailure: () => {
          throw cause;
        },
      }),
    (error: unknown) => error === cause,
  );
});

await test("oversized bigint seeds fail before decimal conversion", () => {
  const script = `
import assert from "node:assert/strict";
const { FixtureFactory } = await import(process.argv[1]);
const factory = FixtureFactory.fromSchema({ type: "integer" });
assert.throws(() => factory.generate({ seed: 1n << 64000000n }), RangeError);
`;
  const entry = new URL("../src/index.js", import.meta.url).href;
  const result = spawnSync(process.execPath, ["--input-type=module", "-e", script, entry], {
    encoding: "utf8",
    timeout: 5000,
  });
  assert.equal(result.status, 0, result.stderr);
});

await test("malformed control containers cannot become default generation or minimization", () => {
  const script = `
import assert from "node:assert/strict";
const { FixtureFactory } = await import(process.argv[1]);
const factory = FixtureFactory.fromSchema({ const: 7 });
for (const options of [
  [], new Date(0), new Number(1),
  Object.setPrototypeOf([], null), Object.setPrototypeOf([], Object.prototype),
]) {
  assert.throws(() => factory.generate(options), RangeError);
  options.isFailure = () => true;
  assert.throws(() => factory.minimize(7, options), RangeError);
}
assert.deepEqual(factory.generate({ seed: 42n }), [7]);
assert.deepEqual(factory.generate(Object.create(null)), [7]);
`;
  const entry = new URL("../src/index.js", import.meta.url).href;
  const result = spawnSync(process.execPath, ["--input-type=module", "-e", script, entry], {
    encoding: "utf8",
    timeout: 5000,
  });
  assert.equal(result.status, 0, result.stderr);
});

await test("minimization preserves method oracle receivers", () => {
  const factory = FixtureFactory.fromSchema({ type: "string", minLength: 1 });
  const options = {
    preserveSchema: true,
    maxEvaluations: 20,
    isFailure(value: unknown): boolean {
      return this.preserveSchema && typeof value === "string" && value.includes("!");
    },
  };
  const result = factory.minimize("a!b", options);
  assert.equal(result.value, "!");
  assert.ok(result.evaluations <= 20);
  assert.equal(result.budgetExhausted, false);
});

await test("provider and factory reference cycles can be collected", () => {
  const script = `
import assert from "node:assert/strict";
import { setImmediate } from "node:timers/promises";
const { FixtureFactory } = await import(process.argv[1]);
function createCycle() {
  let factory;
  const provider = () => factory.isValid("valid") ? "valid" : "invalid";
  factory = FixtureFactory.fromSchema(
    { type: "string", "x-fixture": { provider: "linked" } },
    { providers: { linked: provider } },
  );
  assert.deepEqual(factory.generate({ seed: 42n, profile: "realistic" }), ["valid"]);
  return [new WeakRef(factory), new WeakRef(provider)];
}
const references = createCycle();
for (let attempt = 0; attempt < 20; attempt += 1) {
  await setImmediate();
  global.gc();
  if (references.every(reference => reference.deref() === undefined)) break;
}
assert.ok(references.every(reference => reference.deref() === undefined));
`;
  const entry = new URL("../src/index.js", import.meta.url).href;
  const result = spawnSync(
    process.execPath,
    ["--expose-gc", "--input-type=module", "-e", script, entry],
    { encoding: "utf8", timeout: 5000 },
  );
  assert.equal(result.status, 0, result.stderr);
});

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
