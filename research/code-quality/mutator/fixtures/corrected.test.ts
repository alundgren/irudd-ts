import { expect, it } from 'vitest';
import { canShip, canRelease, addCredits, initialEligibility, serviceFee, normalizeCredits } from './account';

it('ships at the minimum order and rejects an order below it', () => {
  expect(canShip(100)).toBe(true);
  expect(canShip(99)).toBe(false);
  expect(canShip(101)).toBe(true);
});
it('requires both payment and verification', () => {
  expect(canRelease(true, true)).toBe(true);
  expect(canRelease(false, true)).toBe(false);
  expect(canRelease(true, false)).toBe(false);
  expect(canRelease(false, false)).toBe(false);
});
it('adds positive credits and leaves zero credits unchanged', () => {
  expect(addCredits(12, 3)).toBe(15);
  expect(addCredits(12, 0)).toBe(12);
});
it('starts eligible', () => { expect(initialEligibility()).toBe(true); });
it('charges no service fee', () => { expect(serviceFee()).toBe(0); });
it('retains positive and negative integer credits', () => {
  expect(normalizeCredits(12)).toBe(12);
  expect(normalizeCredits(-12)).toBe(-12);
});
