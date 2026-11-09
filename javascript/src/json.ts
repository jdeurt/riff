import { readFileSync } from "node:fs";

import { parse, toSafeNumberOrThrow } from "lossless-json";
import { parseDocument, visit } from "yaml";

import { GenerationError, SchemaError } from "./errors.js";
import type { JSONObject, JSONValue } from "./types.js";

const MAX_JSON_DEPTH = 128;

/** Validate untrusted JSON/YAML/provider output, isolating it from its owner. */
export function jsonValue(
  input: unknown,
  ancestors: ReadonlySet<object> = new Set(),
  depth = 0,
): JSONValue {
  if (depth > MAX_JSON_DEPTH) {
    throw new SchemaError("JSON nesting exceeds 128 levels.");
  }
  if (input === null || typeof input === "string" || typeof input === "boolean") {
    return input;
  }
  if (typeof input === "number" && Number.isFinite(input)) {
    return input;
  }
  if (typeof input !== "object") {
    throw new SchemaError("Expected finite JSON data.");
  }
  return copyContainer(input, ancestors, depth);
}

function copyContainer(input: object, ancestors: ReadonlySet<object>, depth: number): JSONValue {
  if (ancestors.has(input)) {
    throw new SchemaError("JSON data must be acyclic.");
  }
  const nested = new Set(ancestors).add(input);
  if (Array.isArray(input)) {
    return Array.from(input, (item: unknown) => jsonValue(item, nested, depth + 1));
  }
  return copyObject(input, nested, depth);
}

function copyObject(input: object, ancestors: ReadonlySet<object>, depth: number): JSONObject {
  if (Object.getPrototypeOf(input) !== Object.prototype && Object.getPrototypeOf(input) !== null) {
    throw new SchemaError("JSON objects must have a plain prototype.");
  }
  if (Reflect.ownKeys(input).length !== Object.keys(input).length) {
    throw new SchemaError("Expected enumerable string JSON keys.");
  }
  return Object.fromEntries(
    Object.keys(input).map((key) => {
      const descriptor = Object.getOwnPropertyDescriptor(input, key);
      if (
        descriptor === undefined ||
        descriptor.get !== undefined ||
        descriptor.set !== undefined
      ) {
        throw new SchemaError("JSON properties must contain data, not accessors.");
      }
      const value: unknown = descriptor.value;
      return [key, jsonValue(value, ancestors, depth + 1)];
    }),
  );
}

export function readDocument(path: string): JSONValue {
  try {
    const document = parseDocument(readFileSync(path, "utf8"), {
      uniqueKeys: true,
      intAsBigInt: true,
    });
    if (document.errors.length > 0) {
      throw new SchemaError(document.errors.map((error) => error.message).join("; "));
    }
    visit(document, {
      Scalar(_key, scalar) {
        if (typeof scalar.value === "bigint") {
          scalar.value = toSafeNumberOrThrow(scalar.value.toString());
        } else if (typeof scalar.value === "number") {
          if (scalar.source === undefined) {
            throw new SchemaError("Numeric YAML values require their source text.");
          }
          scalar.value = toSafeNumberOrThrow(scalar.source.replaceAll("_", ""));
        }
      },
    });
    const input: unknown = document.toJS({ maxAliasCount: 0 });
    return jsonValue(input);
  } catch (error) {
    throw new SchemaError(`Unable to read ${path}.`, { cause: error });
  }
}

/** Decode native JSON without silently changing a schema-valid numeric value. */
export function decodeNative(payload: string): JSONValue {
  try {
    return jsonValue(parse(payload, undefined, toSafeNumberOrThrow));
  } catch (error) {
    throw new GenerationError("Native JSON contains a number JavaScript cannot preserve.", {
      cause: error,
    });
  }
}
