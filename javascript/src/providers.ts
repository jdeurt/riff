import { ProviderError } from "./errors.js";
import { jsonValue } from "./json.js";
import type { ProviderRequest } from "./native.js";
import type { Provider } from "./types.js";

/** Adapt custom callbacks with isolated contexts and lossless seeds. */
export function providerBridge(
  providers: ReadonlyMap<string, Provider>,
): (request: ProviderRequest) => string {
  return (request: ProviderRequest): string => {
    try {
      const provider = providers.get(request.name);
      if (provider === undefined) {
        throw new ProviderError(`Unknown registered provider: ${request.name}`);
      }
      const value = provider({
        seed: BigInt(request.seed),
        locale: request.locale,
        schema: request.schema,
        options: request.options,
      });
      return JSON.stringify(jsonValue(value));
    } catch (error) {
      throw new ProviderError(`Provider ${request.name} failed.`, { cause: error });
    }
  };
}
