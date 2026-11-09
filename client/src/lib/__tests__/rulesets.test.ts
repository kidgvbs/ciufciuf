import { describe, it, expect } from 'vitest';
import { RULESETS, DEFAULT_RULESET_ID } from '../rulesets';

describe('catalogo ruleset', () => {
  it('ha id univoci', () => {
    const ids = RULESETS.map((r) => r.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('il default esiste ed è disponibile', () => {
    const def = RULESETS.find((r) => r.id === DEFAULT_RULESET_ID);
    expect(def).toBeDefined();
    expect(def!.available).toBe(true);
  });

  it('almeno un ruleset è creabile', () => {
    expect(RULESETS.some((r) => r.available)).toBe(true);
  });

  it('usa_v2 (unico ruleset server in data/rulesets) è presente', () => {
    expect(RULESETS.some((r) => r.id === 'usa_v2')).toBe(true);
  });
});
