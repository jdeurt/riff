import { Faker, de, en, en_GB, en_US, fr, ja } from "@faker-js/faker";
import type { LocaleDefinition } from "@faker-js/faker";
import { z } from "zod";

import { ProviderError } from "./errors.js";
import { jsonValue } from "./json.js";
import type { JSONValue, Provider } from "./types.js";

const LOCALES: Readonly<Record<string, LocaleDefinition>> = {
  "en-US": en_US,
  "en-GB": en_GB,
  "de-DE": de,
  "fr-FR": fr,
  "ja-JP": ja,
};
const requestModel = z.strictObject({
  kind: z.enum(["provider", "format"]),
  name: z.string(),
  seed: z.string().regex(/^\d+$/u),
  locale: z.string(),
  schema: z.record(z.string(), z.json()),
  options: z.record(z.string(), z.json()),
});
export const supportedLocales: readonly string[] = Object.keys(LOCALES);

type NativeRequest = z.infer<typeof requestModel>;
type Generator = (faker: Faker) => string;
const SEED_WORD_BASE = 4_294_967_296n;
const DATE_START = 0;
const DATE_END = 10;
const TIME_START = 11;
const TIME_END = 19;
const DATES = { from: "2000-01-01T00:00:00Z", to: "2030-01-01T00:00:00Z" };
const SEMANTIC: Readonly<Record<string, Generator>> = {
  email: (faker) => faker.internet.email(),
  "full-name": (faker) => faker.person.fullName(),
  "company-name": (faker) => faker.company.name(),
  url: (faker) => faker.internet.url(),
  "phone-number": (faker) => faker.phone.number(),
  city: (faker) => faker.location.city(),
  "country-code": (faker) => faker.location.countryCode(),
  uuid: (faker) => faker.string.uuid(),
};
const FORMATTED: Readonly<Record<string, Generator>> = {
  email: (faker) => faker.internet.email(),
  "idn-email": (faker) => faker.internet.email(),
  uuid: (faker) => faker.string.uuid(),
  uri: (faker) => faker.internet.url(),
  "uri-reference": (faker) => faker.internet.url(),
  iri: (faker) => faker.internet.url(),
  "iri-reference": (faker) => faker.internet.url(),
  hostname: (faker) => faker.internet.domainName(),
  "idn-hostname": (faker) => faker.internet.domainName(),
  ipv4: (faker) => faker.internet.ipv4(),
  ipv6: (faker) => faker.internet.ipv6(),
  date: (faker) => faker.date.between(DATES).toISOString().slice(DATE_START, DATE_END),
  "date-time": (faker) => faker.date.between(DATES).toISOString(),
  time: (faker) => `${faker.date.between(DATES).toISOString().slice(TIME_START, TIME_END)}Z`,
  duration: (faker) => `P${faker.number.int({ min: 0, max: 365 })}D`,
  "json-pointer": () => "/name",
  "relative-json-pointer": () => "0/name",
  "uri-template": () => "/customers/{id}",
  regex: () => "[A-Z]+",
  color: () => "#a0b1c2",
};

function seededFaker(request: NativeRequest, fakers: Map<string, Faker>): Faker {
  let faker = fakers.get(request.locale);
  if (faker === undefined) {
    const locale = LOCALES[request.locale];
    if (locale === undefined) {
      throw new ProviderError(`Unsupported locale: ${request.locale}`);
    }
    faker = new Faker({ locale: [locale, en] });
    fakers.set(request.locale, faker);
  }
  const seed = BigInt(request.seed);
  faker.seed([Number(seed / SEED_WORD_BASE), Number(seed % SEED_WORD_BASE)]);
  return faker;
}

function providerValue(
  request: NativeRequest,
  providers: ReadonlyMap<string, Provider>,
  fakers: Map<string, Faker>,
): JSONValue {
  const custom = providers.get(request.name);
  if (request.kind === "provider" && custom !== undefined) {
    return custom({
      seed: BigInt(request.seed),
      locale: request.locale,
      schema: request.schema,
      options: request.options,
    });
  }
  const generator = (request.kind === "format" ? FORMATTED : SEMANTIC)[request.name];
  if (generator === undefined) {
    throw new ProviderError(`Unknown provider: ${request.name}`);
  }
  return generator(seededFaker(request, fakers));
}

/** Own provider state for one native batch; never change global Faker state. */
export function providerBridge(
  providers: ReadonlyMap<string, Provider>,
): (payload: string) => string {
  const fakers = new Map<string, Faker>();
  return (payload: string): string => {
    const input: unknown = JSON.parse(payload);
    const request = requestModel.parse(input);
    try {
      return JSON.stringify(jsonValue(providerValue(request, providers, fakers)));
    } catch (error) {
      throw new ProviderError(`Provider ${request.name} failed.`, { cause: error });
    }
  };
}
