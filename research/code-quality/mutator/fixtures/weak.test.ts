import { expect, it } from 'vitest';
import { canShip, canRelease, addCredits, initialEligibility, serviceFee, normalizeCredits } from './account';

it('ships large orders and rejects small ones', () => {
  expect(canShip(150)).toBe(true);
  expect(canShip(50)).toBe(false);
});
it('releases fully verified payments', () => {
  expect(canRelease(true, true)).toBe(true);
  expect(canRelease(false, false)).toBe(false);
});
it('retains a balance when adding no credits', () => {
  expect(addCredits(12, 0)).toBe(12);
});
it('returns a boolean initial eligibility', () => {
  expect(typeof initialEligibility()).toBe('boolean');
});
it('keeps service fees within the allowance', () => {
  expect(serviceFee()).toBeLessThanOrEqual(1);
});
it('normalizes integer credits', () => {
  expect(normalizeCredits(12)).toBe(12);
});
