import type { StandardJSONSchemaV1, StandardSchemaV1 } from "@standard-schema/spec";

/** Finite, serializable JSON data; seeds use a separate type. */
export type JSONValue = null | boolean | number | string | JSONValue[] | JSONObject;
/** String-keyed JSON object, including generated unknown properties. */
export interface JSONObject {
  [key: string]: JSONValue;
}
/** JSON Schema object or boolean contract. */
export type Schema = JSONObject | boolean;
/** Sampling preference; every positive result still respects the schema. */
export type Profile = "mixed" | "realistic" | "boundary";
/** Caller-declared consumer outcome, independent of schema validity. */
export type Expectation = "pass" | "fail";
/** Integer seed normalized modulo 2^64; use bigint beyond safe number range. */
export type Seed = bigint | number;

/** Fresh per-invocation context; mutations cannot alter another invocation. */
export interface ProviderContext {
  /** Lossless unsigned 64-bit seed; use local randomness derived from this value. */
  readonly seed: bigint;
  /** Portable locale spelling such as en-US. */
  readonly locale: string;
  /** Normalized schema at the provider location. */
  readonly schema: Schema;
  /** Isolated provider-specific JSON options. */
  readonly options: JSONObject;
}
/** Synchronous semantic generator returning one finite JSON value. */
export type Provider = (context: ProviderContext) => JSONValue;
/** One diagnostic from the original compiled contract. */
export interface Violation {
  /** RFC 6901 JSON Pointer; empty string denotes the root. */
  readonly path: string;
  /** Diagnostic text; not a stable identifier across engine upgrades. */
  readonly message: string;
}
/** Schema-valid payload and an optional inferred model view. */
export interface SuccessCase<ValueT = JSONValue> {
  /** Validated view, inferred from a bound Standard Schema model. */
  readonly value: ValueT;
  /** Original JSON to send through your test client, including allowed extras. */
  readonly payload: JSONValue;
  /** The caller expects the consumer to accept this input. */
  readonly expectation: "pass";
  /** Literal validity guarantee for successful cases. */
  readonly schema_valid: true;
  /** Successful cases contain no schema diagnostics. */
  readonly violations: readonly [];
  /** Successful cases have no mutation target. */
  readonly target: null;
  /** Successful cases have no failure reason. */
  readonly reason: null;
}
/** Changed JSON with a declared negative outcome; it can still be schema-valid. */
export interface FailureCase {
  /** Raw JSON; malformed failures cannot be represented as validated models. */
  readonly value: JSONValue;
  /** Original JSON to send through your test client, including allowed extras. */
  readonly payload: JSONValue;
  /** Expected rejection by the consumer, including business-rule failures. */
  readonly expectation: "fail";
  /** Observed validity against the whole original contract. */
  readonly schema_valid: boolean;
  /** Independent whole-payload validation diagnostics. */
  readonly violations: readonly Violation[];
  /** JSON Pointer to the single replaced field. */
  readonly target: string;
  /** Optional explanation from the failure declaration. */
  readonly reason: string | null;
}
/** Discriminated outcome union; narrow with expectation. */
export type FixtureCase<ValueT = JSONValue> = SuccessCase<ValueT> | FailureCase;
/** Per-batch controls; calls never share mutable generator state. */
export interface GenerationOptions {
  /** Batch size, 0..10000; defaults to 1. */
  readonly count?: number;
  /** Omit for nondeterministic generation. */
  readonly seed?: Seed;
  /** Sampling preference; defaults to mixed. */
  readonly profile?: Profile;
  /** Per-node rejection limit, 1..100000; defaults to 100. */
  readonly maxAttempts?: number;
}
/** Compilation-time provider registrations and optional sidecar annotations. */
export interface FactoryOptions {
  /** Custom names may override built-in provider names. */
  readonly providers?: Readonly<Record<string, Provider>>;
  /** Parsed sidecar; mutually exclusive with fixtureFile. */
  readonly fixtures?: JSONValue;
  /** Local JSON/YAML sidecar path. */
  readonly fixtureFile?: string;
}
/** Exact OpenAPI operation body selector; no status fallback. */
export interface Operation {
  /** Exact OpenAPI path template, such as /registrations. */
  readonly path: string;
  /** Case-insensitive HTTP method; defaults to get. */
  readonly method?: string;
  /** Request or response body; defaults to response. */
  readonly body?: "request" | "response";
  /** Exact response status; defaults to 200, ignored for requests. */
  readonly status?: string;
  /** Exact content-map key; defaults to application/json. */
  readonly mediaType?: string;
}
/** Synchronous caller-owned failure oracle and reduction controls. */
export interface MinimizationOptions {
  /** Must return true for the original input; exceptions propagate unchanged. */
  readonly isFailure: (value: JSONValue) => boolean;
  /** Oracle-call budget including initial confirmation; defaults to 100. */
  readonly maxEvaluations?: number;
  /** Require valid inputs and reductions; defaults to true. */
  readonly preserveSchema?: boolean;
}
/** Greedy retained reproducer; not a guarantee of a global minimum. */
export interface MinimizationResult {
  /** Smallest retained reproducer found within the budgets. */
  readonly value: JSONValue;
  /** Actual oracle invocations, including original confirmation. */
  readonly evaluations: number;
  /** Oracle or candidate-work budget prevented further search. */
  readonly budgetExhausted: boolean;
}
/** A model supplying synchronous validation and input JSON Schema, such as Zod 4. */
export type Model<ValueT> = StandardSchemaV1<unknown, ValueT> &
  StandardJSONSchemaV1<unknown, ValueT>;
