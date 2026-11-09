/** Base class for Riff contract, generation, and provider failures. */
export class RiffError extends Error {
  public override name = "RiffError";
}
/** Invalid or unsupported schema, metadata, or operation selection. */
export class SchemaError extends RiffError {
  public override name = "SchemaError";
}
/** Bounded generation or mutation could not produce a complete batch. */
export class GenerationError extends RiffError {
  public override name = "GenerationError";
}
/** Host provider raised or returned a value outside the JSON contract. */
export class ProviderError extends RiffError {
  public override name = "ProviderError";
}
