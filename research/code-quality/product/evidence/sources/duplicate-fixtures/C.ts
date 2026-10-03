export function invoiceTotal(lines: Line[], discount: number): number {
  let total = 0;
  for (const line of lines) {
    total += line.price * line.quantity;
  }
  return Math.max(0, total - discount);
}

export function reducedInvoiceTotal(lines: Line[], discount: number): number {
  const total = lines.reduce((sum, line) => sum + line.price * line.quantity, 0);
  const discounted = total - discount;
  if (discounted < 0) {
    return 0;
  }
  return discounted;
}
