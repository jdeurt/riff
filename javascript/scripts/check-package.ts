import { execFileSync } from "node:child_process";
import { mkdtempSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";

const archives = readdirSync(".").filter(
  (name) => name.startsWith("riff-fixtures-") && name.endsWith(".tgz"),
);
const [archive] = archives;
if (archives.length !== 1 || archive === undefined) {
  throw new Error(
    "Run npm pack and keep exactly one riff-fixtures tarball for the consumer check.",
  );
}
// oxlint-disable-next-line node/no-process-env -- npm supplies its portable CLI entry point for this check.
const npmCLI = process.env["npm_execpath"];
if (npmCLI === undefined) {
  throw new Error("Run this check with npm run check:package.");
}
const directory = mkdtempSync(path.join(os.tmpdir(), "riff-consumer-"));
try {
  writeFileSync(
    path.join(directory, "package.json"),
    JSON.stringify({ type: "module", private: true }),
  );
  execFileSync(
    process.execPath,
    [
      npmCLI,
      "install",
      "--ignore-scripts",
      "--no-audit",
      "--no-fund",
      "--no-package-lock",
      path.resolve(archive),
    ],
    { cwd: directory, stdio: "pipe" },
  );
  writeFileSync(
    path.join(directory, "consumer.ts"),
    `import { FixtureFactory, fixture } from "riff-fixtures";
import type { Model } from "riff-fixtures";

const model: Model<number> = {
  "~standard": {
    version: 1,
    vendor: "package-consumer",
    validate: value => typeof value === "number"
      ? { value }
      : { issues: [{ message: "Expected number" }] },
    jsonSchema: {
      input: () => ({ type: "integer", const: 7, ...fixture({ fail: { examples: [0] } }) }),
      output: () => ({ type: "integer" }),
    },
  },
};
const factory = FixtureFactory.forModel(model);
const values: number[] = factory.generate({ count: 3, seed: 42n });
if (JSON.stringify(values) !== "[7,7,7]") throw new Error("Incorrect model outputs");
const [success] = factory.successCases({ seed: 42n });
if (success === undefined) throw new Error("Missing success case");
const value: number = success.value;
const pass: "pass" = success.expectation;
const valid: true = success.schema_valid;
if (value !== 7 || pass !== "pass" || !valid) throw new Error("Incorrect typed success case");
const [failure] = factory.failureCases({ seed: 42n });
if (failure === undefined || failure.value !== 0 || failure.schema_valid) {
  throw new Error("Incorrect declared failure");
}
`,
  );
  execFileSync(
    process.execPath,
    [
      path.resolve("node_modules/@typescript/native-preview/bin/tsgo"),
      "--strict",
      "--exactOptionalPropertyTypes",
      "--noUncheckedIndexedAccess",
      "--skipLibCheck",
      "false",
      "--target",
      "ES2023",
      "--module",
      "NodeNext",
      "--moduleResolution",
      "NodeNext",
      "--outDir",
      "build",
      "consumer.ts",
    ],
    { cwd: directory, stdio: "inherit" },
  );
  execFileSync(process.execPath, ["build/consumer.js"], { cwd: directory, stdio: "inherit" });
} finally {
  rmSync(directory, { recursive: true, force: true });
}
