export function invoiceTotal(lines: Line[], discount: number): number {
  let total = 0;
  for (const line of lines) {
    total += line.price * line.quantity;
  }
  return Math.max(0, total - discount);
}

export function checkedInvoiceTotal(lines: Line[], discount: number): number {
  if (discount < 0) throw new Error("negative discount");
  audit(lines);
  let total = 0;
  for (const line of lines) {
    total += line.price * line.quantity;
  }
  return Math.max(0, total - discount);
}
