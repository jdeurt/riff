import { callNative, native } from "./native.js";
import type { JSONValue } from "./types.js";

/** Copy finite, plain JSON data for a fixture operation. */
export function jsonValue(input: unknown): JSONValue {
  return callNative(() => native.jsonValue(input));
}
