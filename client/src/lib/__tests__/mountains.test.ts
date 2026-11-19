import { describe, it, expect } from 'vitest';
import { mountainClaimNote, mountainSymbols } from '../mountains';

describe('mountainSymbols', () => {
  it('0 senza X', () => {
    expect(mountainSymbols({})).toBe(0);
    expect(mountainSymbols({ mountains: 2 })).toBe(2);
  });
});

describe('mountainClaimNote', () => {
  it('treni scartati e punti dal ruleset: «⛰ −k treni, +2k punti»', () => {
    expect(mountainClaimNote({ mountains: 2 }, 2)).toBe('⛰ −2 treni, +4 punti');
    expect(mountainClaimNote({ mountains: 3 }, 2)).toBe('⛰ −3 treni, +6 punti');
  });

  it('usa i punti per treno del ruleset, non un valore fisso', () => {
    expect(mountainClaimNote({ mountains: 2 }, 3)).toBe('⛰ −2 treni, +6 punti');
  });

  it('singolare con una sola X e un punto', () => {
    expect(mountainClaimNote({ mountains: 1 }, 1)).toBe('⛰ −1 treno, +1 punto');
    expect(mountainClaimNote({ mountains: 1 }, 2)).toBe('⛰ −1 treno, +2 punti');
  });

  it('catalogo non pronto: solo i treni', () => {
    expect(mountainClaimNote({ mountains: 2 }, null)).toBe('⛰ −2 treni');
  });

  it('nessuna nota su una rotta senza X', () => {
    expect(mountainClaimNote({}, 2)).toBeNull();
    expect(mountainClaimNote({ mountains: 0 }, 2)).toBeNull();
  });
});
