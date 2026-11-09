import assert from "node:assert/strict";
import { test } from "node:test";

import { FixtureFactory } from "../src/index.js";

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
