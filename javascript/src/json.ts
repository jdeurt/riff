import { native } from "./native.js";
import type { JSONValue } from "./types.js";

/** Copy finite, plain JSON data for a model operation. */
export const jsonValue: (input: unknown) => JSONValue = native.jsonValue;
