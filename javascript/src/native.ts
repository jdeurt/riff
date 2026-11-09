import { GenerationError, ProviderError, RiffError, SchemaError } from "./errors.js";

const DOMAIN_ERRORS: ReadonlyMap<
  string,
  new (message: string, options: ErrorOptions) => RiffError
> = new Map([
  ["SchemaError", SchemaError],
  ["GenerationError", GenerationError],
  ["ProviderError", ProviderError],
]);

/** Translate native domain errors while preserving user provider/oracle exceptions. */
export function callNative<ValueT>(
  operation: () => ValueT,
  callbackErrors: readonly unknown[] = [],
): ValueT {
  try {
    return operation();
  } catch (error) {
    if (callbackErrors.some((callbackError) => Object.is(error, callbackError))) {
      throw error;
    }
    if (error instanceof RiffError || !(error instanceof Error)) {
      throw error;
    }
    const DomainError = DOMAIN_ERRORS.get(error.name);
    if (DomainError === undefined) {
      throw error;
    }
    throw new DomainError(error.message, { cause: error });
  }
}

export { default as native } from "./binding.cjs";
export type { FactoryHandle, ProviderRequest } from "../../native/index.node";
