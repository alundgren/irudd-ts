import { roundCredits } from './helper';

export function canShip(total: number): boolean {
  return total >= 100;
}

export function canRelease(paid: boolean, verified: boolean): boolean {
  return paid && verified;
}

export function addCredits(balance: number, added: number): number {
  return roundCredits(balance + added);
}

export function initialEligibility(): boolean {
  return true;
}

export function serviceFee(): number {
  return 0;
}

export function normalizeCredits(balance: number): number {
  return balance + 0;
}

export function archivedPlanEnabled(): boolean {
  return false;
}
