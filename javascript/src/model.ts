import type { StandardSchemaV1 } from "@standard-schema/spec";

import { GenerationError, SchemaError } from "./errors.js";
import type { FixtureFactory } from "./factory.js";
import { jsonValue } from "./json.js";
import type { FailureCase, GenerationOptions, JSONValue, SuccessCase } from "./types.js";

/** Typed model views preserve original JSON; negative cases remain raw. */
export class ModelFactory<ValueT> {
  private readonly factory: FixtureFactory;
  private readonly model: StandardSchemaV1<unknown, ValueT>;
  public constructor(factory: FixtureFactory, model: StandardSchemaV1<unknown, ValueT>) {
    this.factory = factory;
    this.model = model;
  }
  /** Return inferred model values after original-schema and model validation. */
  public generate(options: GenerationOptions = {}): ValueT[] {
    return this.factory.generate(options).map((value) => this.decode(value));
  }
  /** Retain typed values and their exact original JSON independently. */
  public successCases(options: GenerationOptions = {}): SuccessCase<ValueT>[] {
    return this.factory.successCases(options).map((caseValue) => ({
      value: this.decode(caseValue.value),
      payload: caseValue.payload,
      expectation: caseValue.expectation,
      schema_valid: caseValue.schema_valid,
      violations: caseValue.violations,
      target: caseValue.target,
      reason: caseValue.reason,
    }));
  }
  /** Malformed values cannot be represented as validated models. */
  public failureCases(options: GenerationOptions = {}): FailureCase[] {
    return this.factory.failureCases(options);
  }
  private decode(value: JSONValue): ValueT {
    const result = this.model["~standard"].validate(structuredClone(value));
    if ("then" in result) {
      throw new SchemaError("Model validators must be synchronous.");
    }
    if (result.issues !== undefined) {
      throw new GenerationError(
        `Generated value violates bound model: ${result.issues.map((issue) => issue.message).join("; ")}`,
      );
    }
    // oxlint-disable-next-line unicorn/prefer-structured-clone -- Validate JSON model serialization, including toJSON output.
    const serialized: unknown = JSON.parse(JSON.stringify(result.value));
    if (!this.factory.isValid(jsonValue(serialized))) {
      throw new GenerationError("Serialized bound model violates the original schema.");
    }
    return result.value;
  }
}
