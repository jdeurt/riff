import { z } from "zod";

import { jsonValue } from "./json.js";
import { callNative, native } from "./native.js";
import type { JSONObject } from "./types.js";
/** Return a clean OpenAPI copy, retaining refs, constraints, and literal payload data. */
export function stripFixtureMetadata(document: JSONObject): JSONObject {
  const output = callNative(() => native.stripMetadata(JSON.stringify(jsonValue(document))));
  const parsed: unknown = JSON.parse(output);
  return z.record(z.string(), z.json()).parse(parsed);
}
