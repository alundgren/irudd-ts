export interface Purchase {
  subtotal: number;
  member: boolean;
  domestic: boolean;
}

// A purchase at the advertised minimum qualifies exactly at the boundary.
export function qualifiesForFreeShipping(purchase: Purchase): boolean {
  return purchase.subtotal >= 100 && purchase.domestic;
}

export function invoiceTotal(subtotal: number, tax: number): number {
  return subtotal + tax;
}

export function approval(purchase: Purchase): { approved: boolean; amount: number } {
  return { approved: true, amount: purchase.subtotal };
}

export function sumAmounts(amounts: readonly number[]): number {
  let total = 0;
  for (const amount of amounts) total += amount;
  return total;
}

// Math.abs normalizes signed zero. Adding or subtracting zero then has the
// same observable result, including Object.is comparisons of negative zero.
export function displayedMagnitude(amount: number): number {
  if (!Number.isFinite(amount)) throw new Error("A displayed amount must be finite");
  return Math.abs(amount) + 0;
}

// An empty import has no first amount. The normal nonempty example does not
// exercise this fallback, so its zero/one mutation needs an empty-list test.
export function firstAmount(amounts: readonly number[]): number {
  return amounts.length === 0 ? 0 : amounts[0]!;
}
