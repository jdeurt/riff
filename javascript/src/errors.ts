import { native } from "./native.js";

/** Base class for contract, generation, and provider failures. */
export import RiffError = native.RiffError;
/** Invalid or unsupported contract. */
export import SchemaError = native.SchemaError;
/** Bounded generation failed. */
export import GenerationError = native.GenerationError;
/** Provider failure retaining the caller's original cause. */
export import ProviderError = native.ProviderError;
