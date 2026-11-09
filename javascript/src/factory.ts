import type { StandardSchemaV1 } from "@standard-schema/spec";
import { z } from "zod";

import { success } from "./contracts.js";
import { SchemaError } from "./errors.js";
import { decodeNative, jsonValue, readDocument } from "./json.js";
import { ModelFactory } from "./model.js";
import type { FactoryHandle } from "./native.js";
import { callNative, native } from "./native.js";
import { providerBridge, supportedLocales } from "./providers.js";
import type {
  FactoryOptions,
  FailureCase,
  FixtureCase,
  GenerationOptions,
  JSONObject,
  JSONValue,
  MinimizationOptions,
  MinimizationResult,
  Model,
  Operation,
  Provider,
  Schema,
  SuccessCase,
} from "./types.js";

const generationModel = z.strictObject({
  count: z.number().int().nonnegative().optional(),
  profile: z.enum(["mixed", "realistic", "boundary"]).optional(),
  maxAttempts: z.number().int().positive().optional(),
  seed: z.union([z.bigint(), z.number().int().refine(Number.isSafeInteger)]).optional(),
});
const violationModel = z.strictObject({ path: z.string(), message: z.string() });
const SEED_BITS = 64;
const DEFAULT_MAX_EVALUATIONS = 100;
const failureModel = z.strictObject({
  value: z.json(),
  expectation: z.literal("fail"),
  schema_valid: z.boolean(),
  violations: z.array(violationModel),
  target: z.string(),
  reason: z.string().nullable(),
});

function optionsJSON(options: GenerationOptions): string {
  const parsed = generationModel.parse(options);
  return JSON.stringify({
    ...parsed,
    seed:
      parsed.seed === undefined ? null : BigInt.asUintN(SEED_BITS, BigInt(parsed.seed)).toString(),
  });
}
/** File input with optional exact OpenAPI selection. */
export interface FileOptions extends FactoryOptions, Partial<Operation> {}

/** Compile once; every call uses independent native randomness and provider state. */
export class FixtureFactory {
  private readonly handle: FactoryHandle;
  private readonly providers: ReadonlyMap<string, Provider>;
  private constructor(document: JSONValue, options: FactoryOptions, operation?: Operation) {
    if (options.fixtures !== undefined && options.fixtureFile !== undefined) {
      throw new SchemaError("Choose fixtures or fixtureFile, not both.");
    }
    this.providers = new Map(Object.entries(options.providers ?? {}));
    for (const provider of this.providers.values()) {
      if (typeof provider !== "function") {
        throw new SchemaError("Providers must be synchronous functions.");
      }
    }
    const rawFixtures =
      options.fixtureFile === undefined
        ? (options.fixtures ?? null)
        : readDocument(options.fixtureFile);
    const fixtures = jsonValue(rawFixtures);
    this.handle = callNative(() =>
      native.compile(
        JSON.stringify(jsonValue(document)),
        JSON.stringify({
          providers: [...this.providers.keys()],
          locales: supportedLocales,
          operation: operation ?? null,
          fixtures,
        }),
      ),
    );
  }
  /** Select a supported dialect; default is 2020-12. Unknown validation keywords fail. */
  public static fromSchema(schema: Schema, options: FactoryOptions = {}): FixtureFactory {
    return new FixtureFactory(schema, options);
  }
  /** Select an exact status/media type; missing selections never fall back. */
  public static fromOpenAPI(
    document: JSONObject,
    operation: Operation,
    options: FactoryOptions = {},
  ): FixtureFactory {
    return new FixtureFactory(document, options, operation);
  }
  /** Read local JSON/YAML without fetching references or calling APIs. */
  public static fromFile(path: string, options: FileOptions = {}): FixtureFactory {
    const document = readDocument(path);
    if (options.path !== undefined) {
      if (document === null || typeof document !== "object" || Array.isArray(document)) {
        throw new SchemaError("OpenAPI document must be an object.");
      }
      return FixtureFactory.fromOpenAPI(
        document,
        {
          path: options.path,
          method: options.method ?? "get",
          body: options.body ?? "response",
          status: options.status ?? "200",
          mediaType: options.mediaType ?? "application/json",
        },
        options,
      );
    }
    if (
      typeof document !== "boolean" &&
      (document === null || typeof document !== "object" || Array.isArray(document))
    ) {
      throw new SchemaError("Standalone schema must be an object or boolean.");
    }
    return FixtureFactory.fromSchema(document, options);
  }
  /** Infer model types using Standard Schema and Standard JSON Schema, including Zod 4. */
  public static forModel<ValueT>(
    model: Model<ValueT>,
    options: FactoryOptions = {},
  ): ModelFactory<ValueT> {
    const document = model["~standard"].jsonSchema.input({ target: "draft-2020-12" });
    const schema = jsonValue(Object.fromEntries(Object.entries(document)));
    if (schema === null || typeof schema !== "object" || Array.isArray(schema)) {
      throw new SchemaError("Model must produce a schema object.");
    }
    return FixtureFactory.fromSchema(schema, options).asModel(model);
  }
  /** Bind a synchronous validated view, retaining original payloads. */
  public asModel<ValueT>(model: StandardSchemaV1<unknown, ValueT>): ModelFactory<ValueT> {
    return new ModelFactory(this, model);
  }
  /** Produce a complete valid JSON batch or throw on exhaustion. */
  public generate(options: GenerationOptions = {}): JSONValue[] {
    const output = callNative(() =>
      native.generate(this.handle, optionsJSON(options), providerBridge(this.providers)),
    );
    const parsed: unknown = decodeNative(output);
    return z.array(z.json()).parse(parsed);
  }
  /** Literal pass/true tags distinguish successful cases from raw failure cases. */
  public successCases(options: GenerationOptions = {}): SuccessCase[] {
    return this.generate(options).map((value) => success(value, value));
  }
  /** Replace exactly one reachable declaration in each valid base fixture. */
  public failureCases(options: GenerationOptions = {}): FailureCase[] {
    const output = callNative(() =>
      native.failureCases(this.handle, optionsJSON(options), providerBridge(this.providers)),
    );
    const parsed: unknown = decodeNative(output);
    return z
      .array(failureModel)
      .parse(parsed)
      .map((caseValue) => ({
        value: caseValue.value,
        payload: caseValue.value,
        expectation: caseValue.expectation,
        schema_valid: caseValue.schema_valid,
        violations: caseValue.violations,
        target: caseValue.target,
        reason: caseValue.reason,
      }));
  }
  /** Keep expected consumer outcome independent of observed schema validity. */
  public generateCases(
    options: GenerationOptions & { readonly expectation?: "pass" | "fail" } = {},
  ): FixtureCase[] {
    const { expectation = "pass", ...generation } = options;
    if (z.enum(["pass", "fail"]).parse(expectation) === "pass") {
      return this.successCases(generation);
    }
    return this.failureCases(generation);
  }
  /** Greedily reduce with a synchronous oracle; schema preservation defaults to true. */
  public minimize(value: JSONValue, options: MinimizationOptions): MinimizationResult {
    const controls = z
      .strictObject({ maxEvaluations: z.number().int().positive(), preserveSchema: z.boolean() })
      .parse({
        maxEvaluations: options.maxEvaluations ?? DEFAULT_MAX_EVALUATIONS,
        preserveSchema: options.preserveSchema ?? true,
      });
    const callbackErrors: unknown[] = [];
    const oracle = (payload: string): boolean => {
      try {
        const input: unknown = decodeNative(payload);
        const result = options.isFailure(jsonValue(input));
        if (typeof result !== "boolean") {
          throw new SchemaError("Failure oracles must return synchronous booleans.");
        }
        return result;
      } catch (error) {
        callbackErrors.push(error);
        throw error;
      }
    };
    const output = callNative(
      () =>
        native.minimize(
          this.handle,
          JSON.stringify(jsonValue(value)),
          JSON.stringify(controls),
          oracle,
        ),
      callbackErrors,
    );
    const input: unknown = decodeNative(output);
    const result = z
      .strictObject({
        value: z.json(),
        evaluations: z.number().int().positive(),
        budget_exhausted: z.boolean(),
      })
      .parse(input);
    return {
      value: result.value,
      evaluations: result.evaluations,
      budgetExhausted: result.budget_exhausted,
    };
  }
  /** Validate JSON/model serialization with the original compiled contract. */
  public isValid(value: JSONValue): boolean {
    return callNative(() => native.isValid(this.handle, JSON.stringify(jsonValue(value))));
  }
}
