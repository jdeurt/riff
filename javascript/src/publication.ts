import { jsonValue } from "./json.js";
import { callNative, native } from "./native.js";
import type { JSONObject } from "./types.js";

/** Return a clean OpenAPI copy, retaining refs, constraints, and literal payload data. */
export function stripFixtureMetadata(document: JSONObject): JSONObject {
  return callNative(() => native.stripMetadata(JSON.stringify(jsonValue(document))));
}
