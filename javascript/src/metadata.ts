import { jsonValue } from "./json.js";
import { callNative, native } from "./native.js";
import type { JSONObject, JSONValue, Schema } from "./types.js";

type Values = readonly [JSONValue, ...JSONValue[]];
/** Portable generation tree; schema violations are allowed only inside fail. */
export type Strategy =
  | { readonly kind: "schema"; readonly constraints?: "respect" | "violate" }
  | { readonly kind: "values"; readonly values: Values }
  | {
      readonly kind: "provider";
      readonly name: string;
      readonly locale?: string;
      readonly options?: JSONObject;
    }
  | { readonly kind: "one-of"; readonly strategies: readonly [Strategy, ...Strategy[]] }
  | { readonly kind: "filter"; readonly schema: Schema; readonly strategy: Strategy };
interface Shorthand {
  readonly strategy?: never;
  readonly provider?: string;
  readonly locale?: string;
  readonly options?: JSONObject;
  readonly examples?: Values;
}
interface Explicit {
  readonly strategy: Strategy;
  readonly provider?: never;
  readonly locale?: never;
  readonly options?: never;
  readonly examples?: never;
}
/** Shorthand source or explicit strategy; the two forms cannot be mixed. */
export type Source = Shorthand | Explicit;
/** Required negative source with an optional human-readable reason. */
export type Failure = (
  | Explicit
  | (Shorthand & { readonly provider: string })
  | (Shorthand & { readonly examples: Values })
) & { readonly reason?: string };
/** Complete x-fixture annotation, shared by inline schemas and sidecars. */
export type FixtureMetadata = Source & { readonly fail?: Failure };

/** Build and validate portable metadata for a schema or Standard Schema model. */
export function fixture(metadata: FixtureMetadata): JSONObject {
  const value = jsonValue(metadata);
  callNative(() => {
    native.validateMetadata(JSON.stringify(value));
  });
  return { "x-fixture": value };
}
