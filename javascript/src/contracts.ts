import type { JSONValue, SuccessCase } from "./types.js";

/** Construct a positively tagged view after validation. */
export function success<ValueT>(value: ValueT, payload: JSONValue): SuccessCase<ValueT> {
  return {
    value,
    payload,
    expectation: "pass",
    schema_valid: true,
    violations: [],
    target: null,
    reason: null,
  };
}
