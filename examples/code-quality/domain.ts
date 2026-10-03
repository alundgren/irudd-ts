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

// Subtracting zero instead of adding it changes no observable result here.
// This is an intentional equivalent-mutant control, not a demand for a test.
export function displayedAmount(amount: number): number {
  return amount + 0;
}
