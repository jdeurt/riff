import type { StandardSchemaV1 } from "@standard-schema/spec";

import { SchemaError } from "./errors.js";
import { jsonValue } from "./json.js";
import { ModelFactory } from "./model.js";
import type { FactoryHandle } from "./native.js";
import { callNative, native } from "./native.js";
import { providerBridge } from "./providers.js";
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

function optionsJSON(options: object): string {
  return callNative(() => native.encodeOptions(options));
}
/** File input with optional exact OpenAPI selection. */
export interface FileOptions extends FactoryOptions, Partial<Operation> {}

/** Compile once and generate independent, seeded fixture batches. */
export class FixtureFactory {
  private readonly handle: FactoryHandle;
  private readonly providers: ReadonlyMap<string, Provider>;
  private constructor(source: JSONObject, options: FactoryOptions, operation?: Operation) {
    this.providers = new Map(Object.entries(options.providers ?? {}));
    for (const provider of this.providers.values()) {
      if (typeof provider !== "function") {
        throw new SchemaError("Providers must be synchronous functions.");
      }
    }
    const fixtures =
      options.fixtures === undefined
        ? null
        : { kind: "inline", value: jsonValue(options.fixtures) };
    this.handle = callNative(() =>
      native.compile(
        JSON.stringify(jsonValue(source)),
        JSON.stringify({
          providers: [...this.providers.keys()],
          operation: operation ?? null,
          fixtures,
          fixtureFile: options.fixtureFile,
        }),
      ),
    );
  }

  /** Select a supported dialect; default is 2020-12. Unknown validation keywords fail. */
  public static fromSchema(schema: Schema, options: FactoryOptions = {}): FixtureFactory {
    return new FixtureFactory({ kind: "inline", value: schema }, options);
  }
  /** Select an exact status/media type; missing selections never fall back. */
  public static fromOpenAPI(
    document: JSONObject,
    operation: Operation,
    options: FactoryOptions = {},
  ): FixtureFactory {
    return new FixtureFactory({ kind: "inline", value: document }, options, operation);
  }
  /** Read local JSON/YAML without fetching references or calling APIs. */
  public static fromFile(path: string, options: FileOptions = {}): FixtureFactory {
    const operation =
      options.path === undefined
        ? undefined
        : {
            path: options.path,
            ...(options.method === undefined ? {} : { method: options.method }),
            ...(options.body === undefined ? {} : { body: options.body }),
            ...(options.status === undefined ? {} : { status: options.status }),
            ...(options.mediaType === undefined ? {} : { mediaType: options.mediaType }),
          };
    return new FixtureFactory({ kind: "file", value: path }, options, operation);
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
    return callNative(() =>
      native.generate(this.handle, optionsJSON(options), providerBridge(this.providers)),
    );
  }
  /** Literal pass/true tags distinguish successful cases from raw failure cases. */
  public successCases(options: GenerationOptions = {}): SuccessCase[] {
    return callNative(() =>
      native.successCases(this.handle, optionsJSON(options), providerBridge(this.providers)),
    );
  }
  /** Replace exactly one reachable declaration in each valid base fixture. */
  public failureCases(options: GenerationOptions = {}): FailureCase[] {
    return callNative(() =>
      native.failureCases(this.handle, optionsJSON(options), providerBridge(this.providers)),
    );
  }
  /** Keep expected consumer outcome independent of observed schema validity. */
  public generateCases(
    options: GenerationOptions & { readonly expectation?: "pass" | "fail" } = {},
  ): FixtureCase[] {
    return callNative(() =>
      native.cases(this.handle, optionsJSON(options), providerBridge(this.providers)),
    );
  }
  /** Greedily reduce with a synchronous oracle; schema preservation defaults to true. */
  public minimize(value: JSONValue, options: MinimizationOptions): MinimizationResult {
    const callbackErrors: unknown[] = [];
    const oracle = (candidate: JSONValue): boolean => {
      try {
        const result = options.isFailure(candidate);
        if (typeof result !== "boolean") {
          throw new SchemaError("Failure oracles must return synchronous booleans.");
        }
        return result;
      } catch (error) {
        callbackErrors.push(error);
        throw error;
      }
    };
    const result = callNative(
      () =>
        native.minimize(
          this.handle,
          JSON.stringify(jsonValue(value)),
          optionsJSON({
            maxEvaluations: options.maxEvaluations,
            preserveSchema: options.preserveSchema,
          }),
          oracle,
        ),
      callbackErrors,
    );
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
