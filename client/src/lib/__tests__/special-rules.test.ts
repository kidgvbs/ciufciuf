import { describe, it, expect, beforeEach } from 'vitest';
import { autoOpensRules, hasSeenRules, markRulesSeen } from '../special-rules';

// Stub minimale di localStorage (environment node).
function makeStorage() {
  const map = new Map<string, string>();
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
  };
}

beforeEach(() => {
  (globalThis as Record<string, unknown>).localStorage = makeStorage();
});

describe('regole viste per partita', () => {
  it('di default non sono viste; dopo la chiusura restano viste solo per quella partita', () => {
    expect(hasSeenRules('g1')).toBe(false);
    markRulesSeen('g1');
    expect(hasSeenRules('g1')).toBe(true);
    expect(hasSeenRules('g2')).toBe(false);
  });

  it('gameId vuoto: mai viste, nessuna scrittura', () => {
    markRulesSeen('');
    expect(hasSeenRules('')).toBe(false);
  });

  it('localStorage che lancia non rompe nulla', () => {
    const boom = () => {
      throw new Error('storage bloccato');
    };
    (globalThis as Record<string, unknown>).localStorage = { getItem: boom, setItem: boom };
    expect(() => markRulesSeen('g1')).not.toThrow();
    expect(hasSeenRules('g1')).toBe(false);
  });
});

describe('autoOpensRules', () => {
  it('si apre da solo solo nel draft iniziale, con regole e non ancora viste', () => {
    expect(autoOpensRules(3, 'INITIAL_DRAFT', false)).toBe(true);
    expect(autoOpensRules(0, 'INITIAL_DRAFT', false)).toBe(false);
    expect(autoOpensRules(3, 'INITIAL_DRAFT', true)).toBe(false);
    expect(autoOpensRules(3, 'PLAYER_TURN', false)).toBe(false);
  });
});
