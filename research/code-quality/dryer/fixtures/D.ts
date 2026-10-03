export function outstandingBalance(account: Account): number {
  const gross = account.debit + account.fee;
  const net = gross - account.payment;
  return Math.max(0, net);
}

export function remainingBattery(device: Device): number {
  const gross = device.capacity + device.reserve;
  const net = gross - device.consumed;
  return Math.max(0, net);
}
