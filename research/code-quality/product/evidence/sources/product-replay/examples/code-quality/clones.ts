interface SummaryInput {
  lines: readonly { amount: number; approved: boolean }[];
  enabled: boolean;
}
function round(value: number): number { return Math.round(value); }

// Authored copy/paste controls. Names and literal values change while calls,
// property accesses and most decisions remain the same.
export function invoiceSummary(input: SummaryInput) {
  const results: number[] = [];
  let total = 0;
  let accepted = 0;
  let rejected = 0;
  for (const line of input.lines) {
    if (!line.approved) {
      rejected++;
      continue;
    }
    if (line.amount < 0) {
      rejected++;
      continue;
    }
    const amount = round(line.amount);
    if (amount > 100) {
      results.push(amount - 5);
    } else {
      results.push(amount);
    }
    total += amount;
    accepted++;
  }
  if (!input.enabled) {
    return { results: [], total: 0, accepted: 0, rejected };
  }
  if (accepted === 0) {
    return { results, total, accepted, rejected };
  }
  return { results, total: round(total), accepted, rejected };
}

export function orderSummary(purchase: SummaryInput) {
  const entries: number[] = [];
  let sum = 0;
  let good = 0;
  let bad = 0;
  for (const row of purchase.lines) {
    if (!row.approved) {
      bad++;
      continue;
    }
    if (row.amount < 0) {
      bad++;
      continue;
    }
    const value = round(row.amount);
    if (value > 200) {
      entries.push(value - 10);
    } else {
      entries.push(value);
    }
    sum += value;
    good++;
  }
  if (!purchase.enabled) {
    return { results: [], total: 0, accepted: 0, rejected: bad };
  }
  if (good === 0) {
    return { results: entries, total: sum, accepted: good, rejected: bad };
  }
  return { results: entries, total: round(sum), accepted: good, rejected: bad };
}

// Parallel protocol adapters can intentionally have identical control flow.
export function readPort(value: string): number {
  const parsed = Number(value);
  if (!Number.isInteger(parsed)) throw new Error("Invalid port");
  if (parsed < 1) throw new Error("Port is too small");
  return parsed;
}
export function readRetryCount(value: string): number {
  const parsed = Number(value);
  if (!Number.isInteger(parsed)) throw new Error("Invalid retry count");
  if (parsed < 0) throw new Error("Retry count is too small");
  return parsed;
}
