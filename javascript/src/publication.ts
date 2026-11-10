import { native } from "./native.js";
import type { JSONObject } from "./types.js";

/** Return a clean OpenAPI copy, retaining constraints and literal payload data. */
export const stripFixtureMetadata: (document: JSONObject) => JSONObject =
  native.stripFixtureMetadata;
