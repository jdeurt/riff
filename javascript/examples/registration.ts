import { z } from "zod";

import { fixture } from "../src/index.js";

const MAX_NAME_LENGTH = 64;
const MAX_SEATS = 50;

/** A registration reserves 1–50 seats at 1200 cents per seat. */
export const Registration: z.ZodObject<{
  name: z.ZodString;
  seats: z.ZodNumber;
}> = z.strictObject({
  name: z
    .string()
    .min(1)
    .max(MAX_NAME_LENGTH)
    .meta(fixture({ provider: "full-name", fail: { examples: ["", null] } })),
  seats: z
    .number()
    .int()
    .min(1)
    .max(MAX_SEATS)
    .meta(fixture({ fail: { strategy: { kind: "schema", constraints: "violate" } } })),
});

/** Validated registration inferred without an assertion or explicit generic argument. */
export type RegistrationInput = z.infer<typeof Registration>;

const SEAT_PRICE_CENTS = 1200;

/** Real endpoint logic; tests supply requests and assert its observable responses. */
export function register(input: unknown): Response {
  const parsed = Registration.safeParse(input);
  if (!parsed.success) {
    return Response.json({ error: "Invalid registration" }, { status: 422 });
  }
  return Response.json(
    {
      name: parsed.data.name,
      seats: parsed.data.seats,
      total_cents: parsed.data.seats * SEAT_PRICE_CENTS,
    },
    { status: 201 },
  );
}

/** Fetch-compatible request handler, usable through a framework's HTTP interceptor/test client. */
export async function registrationEndpoint(request: Request): Promise<Response> {
  if (request.method !== "POST" || new URL(request.url).pathname !== "/registrations") {
    return Response.json({ error: "Not found" }, { status: 404 });
  }
  const input: unknown = await request.json();
  return register(input);
}
