export function summarizeOrders(orders: Order[], discount: number): Summary {
  const accepted: Order[] = [];
  const rejected: string[] = [];
  let subtotal = 0;
  let tax = 0;
  let shipping = 0;
  let units = 0;
  for (const order of orders) {
    if (!order.enabled) {
      rejected.push(order.id);
      continue;
    }
    if (order.quantity <= 0) {
      rejected.push(order.id);
      continue;
    }
    const price = Math.max(0, order.price);
    const count = Math.floor(order.quantity);
    const amount = price * count;
    subtotal += amount;
    units += count;
    if (order.taxable) {
      tax += amount * 0.2;
    }
    if (order.express) {
      shipping += 10;
    } else {
      shipping += 3;
    }
    accepted.push(order);
  }
  const cappedDiscount = Math.min(discount, subtotal);
  const total = Math.max(0, subtotal - cappedDiscount + tax + shipping);
  const average = units > 0 ? subtotal / units : 0;
  return {
    accepted,
    rejected,
    subtotal,
    tax,
    shipping,
    units,
    total,
    average,
  };
}

export function summarizeCopiedOrders(purchases: Order[], rebate: number): Summary {
  const included: Order[] = [];
  const excluded: string[] = [];
  let baseAmount = 0;
  let tax = 0;
  let shipping = 0;
  let itemCount = 0;
  for (const purchase of purchases) {
    if (!purchase.enabled) {
      excluded.push(purchase.id);
      continue;
    }
    if (purchase.quantity <= 0) {
      excluded.push(purchase.id);
      continue;
    }
    const price = Math.max(0, purchase.price);
    const count = Math.floor(purchase.quantity);
    const amount = price * count;
    baseAmount += amount;
    itemCount += count;
    if (purchase.taxable) {
      tax += amount * 0.25;
    }
    if (purchase.express && purchase.priority) {
      shipping += 10;
    } else {
      shipping += 5;
    }
    included.push(purchase);
  }
  const limitedDiscount = Math.min(rebate, baseAmount);
  const total = Math.max(0, baseAmount - limitedDiscount + tax + shipping);
  const mean = itemCount > 0 ? baseAmount / itemCount : 0;
  return {
    included,
    excluded,
    baseAmount,
    tax,
    shipping,
    itemCount,
    total,
    mean,
  };
}
