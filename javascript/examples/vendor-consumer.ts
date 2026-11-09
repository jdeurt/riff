import { z } from "zod";

const Customer = z.object({
  id: z.string(),
  name: z.string().min(1),
  status: z.enum(["active", "inactive"]),
});

/** The consumer's real HTTP and parsing boundary; Riff supplies only the vendor data. */
export async function customerWelcome(
  customerId: string,
  vendorURL: string,
  transport: typeof fetch = fetch,
): Promise<Response> {
  const response = await transport(
    new URL(`/customers/${encodeURIComponent(customerId)}`, vendorURL),
  );
  const input: unknown = await response.json();
  const customer = Customer.safeParse(input);
  if (!response.ok || !customer.success) {
    return Response.json({ error: "Malformed vendor response" }, { status: 502 });
  }
  if (customer.data.status !== "active") {
    return Response.json({ error: "Inactive customer" }, { status: 409 });
  }
  return Response.json({ id: customer.data.id, message: `Welcome, ${customer.data.name}!` });
}
