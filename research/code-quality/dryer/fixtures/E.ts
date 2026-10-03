export function loadInvoice(client: Client, id: string): Promise<Invoice> {
  const request = { path: "/invoices", id };
  const result = client.get(request);
  return result;
}

export function loadCustomer(client: Client, id: string): Promise<Customer> {
  const request = { path: "/customers", id };
  const result = client.get(request);
  return result;
}
