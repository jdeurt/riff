import { ModelFactory } from "./model.js";
import { native } from "./native.js";
import type { FactoryOptions, Model, Operation } from "./types.js";

/** File input with optional exact OpenAPI selection. */
export interface FileOptions extends FactoryOptions, Partial<Operation> {}

function modelSchema(model: Model<unknown>): object {
  const document = model["~standard"].jsonSchema.input({ target: "draft-2020-12" });
  return Object.fromEntries(Object.entries(document));
}

native.configureModels(ModelFactory, modelSchema);

/** Compile once and generate independent, seeded fixture batches. */
export import FixtureFactory = native.FixtureFactory;
