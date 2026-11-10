/** Public fixture, metadata, and error contracts. */
declare module "*.node" {
  /** Base class for contract, generation, and provider errors. */
  export class RiffError extends Error {}
  /** Invalid or unsupported schema, metadata, or operation selection. */
  export class SchemaError extends RiffError {}
  /** Bounded generation or mutation could not produce a complete batch. */
  export class GenerationError extends RiffError {}
  /** A provider raised or returned a value outside the JSON contract. */
  export class ProviderError extends RiffError {}

  /** Compile once and generate independent, seeded fixture batches. */
  export class FixtureFactory {
    private constructor();
    /** Compile a standalone schema; the default dialect is 2020-12. */
    public static fromSchema(
      schema: import("./types.js").Schema,
      options?: import("./types.js").FactoryOptions,
    ): FixtureFactory;
    /** Select an exact OpenAPI request or response; missing selectors never fall back. */
    public static fromOpenAPI(
      document: import("./types.js").JSONObject,
      operation: import("./types.js").Operation,
      options?: import("./types.js").FactoryOptions,
    ): FixtureFactory;
    /** Read local JSON/YAML and optionally select an OpenAPI body. */
    public static fromFile(
      path: string,
      options?: import("./factory.js").FileOptions,
    ): FixtureFactory;
    /** Infer schema and result types through Standard Schema interfaces. */
    public static forModel<ValueT>(
      model: import("./types.js").Model<ValueT>,
      options?: import("./types.js").FactoryOptions,
    ): import("./model.js").ModelFactory<ValueT>;
    /** Bind a synchronous model view while retaining original JSON payloads. */
    public asModel<ValueT>(
      model: import("@standard-schema/spec").StandardSchemaV1<unknown, ValueT>,
    ): import("./model.js").ModelFactory<ValueT>;
    /** Produce a complete valid batch or throw on bounded search exhaustion. */
    public generate(
      options?: import("./types.js").GenerationOptions,
    ): import("./types.js").JSONValue[];
    /** Return successful cases with literal pass/true tags. */
    public successCases(
      options?: import("./types.js").GenerationOptions,
    ): import("./types.js").SuccessCase[];
    /** Mutate one reachable failure declaration and retain complete diagnostics. */
    public failureCases(
      options?: import("./types.js").GenerationOptions,
    ): import("./types.js").FailureCase[];
    /** Keep declared consumer outcomes separate from observed schema validity. */
    public generateCases(
      options?: import("./types.js").GenerationOptions & {
        readonly expectation?: import("./types.js").Expectation;
      },
    ): import("./types.js").FixtureCase[];
    /** Reduce a reproducer while preserving caller-owned oracle exceptions. */
    public minimize(
      value: import("./types.js").JSONValue,
      options: import("./types.js").MinimizationOptions,
    ): import("./types.js").MinimizationResult;
    /** Check JSON against the original compiled contract. */
    public isValid(value: import("./types.js").JSONValue): boolean;
  }

  /** Connect Standard Schema model construction and input schema extraction. */
  export function configureModels(
    factory: typeof import("./model.js").ModelFactory,
    schema: (model: import("./types.js").Model<unknown>) => object,
  ): void;
  /** Copy finite plain JSON without invoking payload hooks. */
  export function jsonValue(input: unknown): import("./types.js").JSONValue;
  /** Build and validate portable schema metadata. */
  export function fixture(
    metadata: import("./metadata.js").FixtureMetadata,
  ): import("./types.js").JSONObject;
  /** Return a clean publication copy while preserving literal payload data. */
  export function stripFixtureMetadata(
    document: import("./types.js").JSONObject,
  ): import("./types.js").JSONObject;
  /** Complete command result; the entry point only writes streams. */
  export function cli(argv: string[]): {
    readonly stdout: string;
    readonly stderr: string;
    readonly exitCode: number;
  };
}
