/** Typed ABI exported by the Neon addon. All payloads cross as validated JSON. */
declare module "*.node" {
  /** Opaque compiled Rust contract; only compile can construct this handle. */
  export interface FactoryHandle {
    readonly nativeFactory: unique symbol;
  }
  /** Compile and resolve a contract without making network requests. */
  export function compile(document: string, configuration: string): FactoryHandle;
  /** Generate a complete valid batch, invoking only the supplied provider bridge. */
  export function generate(
    handle: FactoryHandle,
    options: string,
    provider: (request: string) => string,
  ): string;
  /** Mutate one reachable failure declaration per valid base fixture. */
  export function failureCases(
    handle: FactoryHandle,
    options: string,
    provider: (request: string) => string,
  ): string;
  /** Minimize with a synchronous caller-owned oracle; propagate its exceptions. */
  export function minimize(
    handle: FactoryHandle,
    value: string,
    options: string,
    oracle: (payload: string) => boolean,
  ): string;
  /** Check model serialization against the compiled original contract. */
  export function isValid(handle: FactoryHandle, value: string): boolean;
  /** Remove schema annotations while retaining literal payload data. */
  export function stripMetadata(document: string): string;
  /** Validate portable source/strategy structure before model publication. */
  export function validateMetadata(metadata: string): void;
}
