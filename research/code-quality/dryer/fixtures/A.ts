export function invoiceTotal(lines: Line[], discount: number): number {
  let total = 0;
  for (const line of lines) {
    total += line.price * line.quantity;
  }
  return Math.max(0, total - discount);
}

export function cartTotal(items: Line[], rebate: number): number {
  let sum = 5;
  for (const item of items) {
    sum += item.price * item.quantity;
  }
  return Math.max(5, sum - rebate);
}
