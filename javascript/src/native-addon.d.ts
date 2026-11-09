/** Typed native ABI. Payload shapes are guaranteed by complete fixture operations. */
declare module "*.node" {
  /** Opaque compiled contract. */
  export interface FactoryHandle {
    readonly nativeFactory: unique symbol;
  }
  /** Isolated callback inputs; the decimal seed retains all 64 bits. */
  export interface ProviderRequest {
    readonly name: string;
    readonly seed: string;
    readonly locale: string;
    readonly schema: import("./types.js").Schema;
    readonly options: import("./types.js").JSONObject;
  }
  /** Retained reproducer and search usage. */
  export interface Minimum {
    readonly value: import("./types.js").JSONValue;
    readonly evaluations: number;
    readonly budget_exhausted: boolean;
  }
  /** Complete CLI result; the entry point only writes streams. */
  export interface CommandOutput {
    readonly stdout: string;
    readonly stderr: string;
    readonly exitCode: number;
  }
  /** Copy finite plain JSON without invoking payload hooks. */
  export function jsonValue(input: unknown): import("./types.js").JSONValue;
  /** Encode host controls without losing nonfinite numbers or bigint seeds. */
  export function encodeOptions(options: object): string;
  /** Compile an inline/file contract and optional operation/sidecar. */
  export function compile(source: string, configuration: string): FactoryHandle;
  /** Generate a complete valid batch. */
  export function generate(
    handle: FactoryHandle,
    options: string,
    provider: (request: ProviderRequest) => string,
  ): import("./types.js").JSONValue[];
  /** Generate positive cases with literal validity guarantees. */
  export function successCases(
    handle: FactoryHandle,
    options: string,
    provider: (request: ProviderRequest) => string,
  ): import("./types.js").SuccessCase[];
  /** Generate explicit negative cases with observed whole-payload validity. */
  export function failureCases(
    handle: FactoryHandle,
    options: string,
    provider: (request: ProviderRequest) => string,
  ): import("./types.js").FailureCase[];
  /** Generate cases with the requested outcome. */
  export function cases(
    handle: FactoryHandle,
    options: string,
    provider: (request: ProviderRequest) => string,
  ): import("./types.js").FixtureCase[];
  /** Minimize with a synchronous caller-owned oracle. */
  export function minimize(
    handle: FactoryHandle,
    value: string,
    options: string,
    oracle: (value: import("./types.js").JSONValue) => boolean,
  ): Minimum;
  /** Validate JSON against the original contract. */
  export function isValid(handle: FactoryHandle, value: string): boolean;
  /** Remove schema annotations while retaining literal payload data. */
  export function stripMetadata(document: string): import("./types.js").JSONObject;
  /** Validate portable annotations before model publication. */
  export function validateMetadata(metadata: string): void;
  /** Execute the command workflow. */
  export function cli(argumentsJSON: string): CommandOutput;
}
