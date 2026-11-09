import { fileURLToPath } from "node:url";

import { FixtureFactory } from "../src/index.js";

const source = fileURLToPath(new URL("../../../conformance/schema.json", import.meta.url));
const HEX_RADIX = 16;
const SEED_HEX_WIDTH = 16;
const factory = FixtureFactory.fromFile(source, {
  providers: {
    "synthetic.id": (context) =>
      `acct_${context.seed.toString(HEX_RADIX).padStart(SEED_HEX_WIDTH, "0")}`,
  },
});
const count = 20;
const seed = 9_223_372_036_854_775_825n;
const passes = factory.generate({ count, seed, profile: "boundary" });
const failures = factory.failureCases({ count, seed, profile: "boundary" }).map((caseValue) => ({
  value: caseValue.value,
  expectation: caseValue.expectation,
  schema_valid: caseValue.schema_valid,
  target: caseValue.target,
  reason: caseValue.reason,
  violations: caseValue.violations,
}));
process.stdout.write(JSON.stringify({ passes, failures }));
