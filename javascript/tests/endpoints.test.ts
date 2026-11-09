import assert from "node:assert/strict";
import { once } from "node:events";
import { createServer } from "node:http";
import { test } from "node:test";

import { z } from "zod";

import { Registration, registrationEndpoint } from "../examples/registration.js";
import { customerWelcome } from "../examples/vendor-consumer.js";
import { FixtureFactory } from "../src/index.js";
import type { JSONValue } from "../src/index.js";

const REGISTRATIONS = FixtureFactory.forModel(Registration);
const RegistrationResponse = z.object({
  name: z.string(),
  seats: z.number(),
  total_cents: z.number(),
});

await test("typed generated registrations preserve submitted fields through the request handler", async () => {
  for (const caseValue of REGISTRATIONS.successCases({ count: 30, seed: 42 })) {
    const response = await registrationEndpoint(
      new Request("http://test/registrations", {
        method: "POST",
        body: JSON.stringify(caseValue.payload),
      }),
    );
    const input: unknown = await response.json();
    const body = RegistrationResponse.parse(input);
    assert.equal(response.status, 201);
    assert.equal(body.name, caseValue.value.name);
    assert.equal(body.seats, caseValue.value.seats);
  }
});

await test("declared malformed registrations return 422 through the request handler", async () => {
  for (const caseValue of REGISTRATIONS.failureCases({ count: 30, seed: 42 })) {
    const response = await registrationEndpoint(
      new Request("http://test/registrations", {
        method: "POST",
        body: JSON.stringify(caseValue.payload),
      }),
    );
    assert.equal(response.status, 422);
  }
});

await test("seven accepted seats cost 8400 cents", async () => {
  const response = await registrationEndpoint(
    new Request("http://test/registrations", {
      method: "POST",
      body: JSON.stringify({ name: "Known", seats: 7 }),
    }),
  );
  const body: unknown = await response.json();
  assert.equal(response.status, 201);
  assert.deepEqual(body, { name: "Known", seats: 7, total_cents: 8400 });
});

const VENDOR_FACTORY = FixtureFactory.fromSchema({
  type: "object",
  required: ["id", "name", "status"],
  properties: {
    id: { type: "string", format: "uuid" },
    name: {
      type: "string",
      minLength: 1,
      "x-fixture": { provider: "full-name", fail: { examples: [null, 42] } },
    },
    status: {
      enum: ["active", "inactive"],
      "x-fixture": {
        strategy: { kind: "values", values: ["active"] },
        fail: { examples: ["inactive"] },
      },
    },
  },
});

const VendorCustomer = z.object({
  id: z.string(),
  name: z.string().min(1),
  status: z.enum(["active", "inactive"]),
});
interface VendorFixture {
  readonly origin: string;
  readonly setBody: (body: JSONValue) => void;
  readonly close: () => Promise<void>;
}
async function startVendor(): Promise<VendorFixture> {
  let body: JSONValue = null;
  const server = createServer((_request, response) => {
    response.setHeader("content-type", "application/json");
    response.end(JSON.stringify(body));
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address();
  if (address === null || typeof address === "string") {
    throw new Error("Expected an isolated TCP test server.");
  }
  return {
    origin: `http://127.0.0.1:${address.port}`,
    setBody: (value) => {
      body = value;
    },
    close: async () => {
      await server[Symbol.asyncDispose]();
    },
  };
}

await test("valid vendor responses pass through real HTTP and consumer parsing", async (context) => {
  const vendor = await startVendor();
  context.after(vendor.close);
  for (const caseValue of VENDOR_FACTORY.asModel(VendorCustomer).successCases({
    count: 10,
    seed: 42,
  })) {
    vendor.setBody(caseValue.payload);
    const response = await customerWelcome(caseValue.value.id, vendor.origin);
    const body: unknown = await response.json();
    assert.equal(response.status, 200);
    assert.deepEqual(body, {
      id: caseValue.value.id,
      message: `Welcome, ${caseValue.value.name}!`,
    });
  }
});

await test("malformed and inactive vendor responses take different consumer error paths", async (context) => {
  const vendor = await startVendor();
  context.after(vendor.close);
  const observed = new Set<string>();
  for (const caseValue of VENDOR_FACTORY.failureCases({ count: 30, seed: 42 })) {
    vendor.setBody(caseValue.payload);
    const response = await customerWelcome("synthetic", vendor.origin);
    observed.add(caseValue.target);
    assert.equal(response.status, caseValue.target === "/name" ? 502 : 409);
    assert.equal(caseValue.schema_valid, caseValue.target === "/status");
  }
  assert.deepEqual(observed, new Set(["/name", "/status"]));
});
