#!/usr/bin/env node
import { parseArgs } from "node:util";

import { z } from "zod";

import { FixtureFactory } from "./factory.js";

const EXIT_FAILURE = 2;
const JSON_INDENT = 2;

try {
  const parsedArguments = parseArgs({
    allowPositionals: true,
    options: {
      count: { type: "string", default: "1" },
      seed: { type: "string" },
      profile: { type: "string", default: "mixed" },
      path: { type: "string" },
      method: { type: "string", default: "get" },
      status: { type: "string", default: "200" },
      body: { type: "string", default: "response" },
      "media-type": { type: "string", default: "application/json" },
      fixtures: { type: "string" },
      expectation: { type: "string", default: "pass" },
      jsonl: { type: "boolean", default: false },
      "max-attempts": { type: "string", default: "100" },
    },
  });
  const [source] = parsedArguments.positionals;
  if (source === undefined || parsedArguments.positionals.length !== 1) {
    throw new Error("Usage: riff <schema.json|schema.yaml> [--path PATH] [--count N] [--seed N]");
  }
  const options = parsedArguments.values;
  const factory = FixtureFactory.fromFile(source, {
    ...(options.path === undefined ? {} : { path: options.path }),
    method: options.method,
    status: options.status,
    body: z.enum(["request", "response"]).parse(options.body),
    mediaType: options["media-type"],
    ...(options.fixtures === undefined ? {} : { fixtureFile: options.fixtures }),
  });
  const generation = {
    count: Number(options.count),
    profile: z.enum(["mixed", "realistic", "boundary"]).parse(options.profile),
    maxAttempts: Number(options["max-attempts"]),
    ...(options.seed === undefined ? {} : { seed: BigInt(options.seed) }),
  };
  const expectation = z.enum(["pass", "fail"]).parse(options.expectation);
  const cases = factory.generateCases({ ...generation, expectation });
  const output = options.jsonl
    ? cases
        .map((value) =>
          JSON.stringify({
            value: value.value,
            expectation: value.expectation,
            schema_valid: value.schema_valid,
            violations: value.violations,
            target: value.target,
            reason: value.reason,
          }),
        )
        .join("\n")
    : JSON.stringify(
        cases.map((value) => value.value),
        null,
        JSON_INDENT,
      );
  process.stdout.write(`${output}\n`);
} catch (error) {
  process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = EXIT_FAILURE;
}
