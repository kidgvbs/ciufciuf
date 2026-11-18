// Campi di pagamento/pesca nel catalogo dei ruleset e nella mappa (Nordic, Switzerland).
import { describe, it, expect } from 'vitest';
import { isMapData } from '../map-loader';
import { isRulesetInfo, rulesetPayment, type RulesetInfo } from '../rulesets';

const base: RulesetInfo = {
  id: 'nordic',
  name: 'Nordic Countries',
  mapId: 'nordic',
  mode: 'Standard',
  modeOrder: 0,
  cities: 40,
  routes: 90,
  tickets: 46,
  minPlayers: 2,
  maxPlayers: 3,
  bonuses: [],
  bigCities: [],
  initialDealt: 5,
  initialKeepMin: 2,
  drawCount: 3,
  drawKeepMin: 1,
  tunnels: true,
  ferries: true,
  stations: false,
};

describe('isRulesetInfo: regole di locomotive e traghetti', () => {
  it('i campi nuovi sono opzionali (server vecchi)', () => {
    expect(isRulesetInfo(base)).toBe(true);
    expect(
      isRulesetInfo({
        ...base,
        locomotivesUsableOn: ['tunnel', 'ferry'],
        ferryAnyCards: 3,
      }),
    ).toBe(true);
  });

  it('rifiuta valori malformati', () => {
    expect(isRulesetInfo({ ...base, locomotivesUsableOn: [] })).toBe(false);
    expect(isRulesetInfo({ ...base, locomotivesUsableOn: ['tunnel', ''] })).toBe(false);
    expect(isRulesetInfo({ ...base, locomotivesUsableOn: 'tunnel' })).toBe(false);
    expect(isRulesetInfo({ ...base, ferryAnyCards: 1 })).toBe(false);
    expect(isRulesetInfo({ ...base, ferryAnyCards: 2.5 })).toBe(false);
  });
});

describe('rulesetPayment', () => {
  const nordic: RulesetInfo = {
    ...base,
    locomotivesUsableOn: ['tunnel', 'ferry'],
    ferryAnyCards: 3,
  };

  it('legge le regole del ruleset dal catalogo', () => {
    expect(rulesetPayment([nordic], 'nordic')).toEqual({
      usableOn: ['tunnel', 'ferry'],
      ferryAnyCards: 3,
    });
  });

  it('regole classiche se il ruleset non le ha, è ignoto o assente', () => {
    const classic = { usableOn: null, ferryAnyCards: null };
    expect(rulesetPayment([base], 'nordic')).toEqual(classic);
    expect(rulesetPayment([nordic], 'altro')).toEqual(classic);
    expect(rulesetPayment([nordic], null)).toEqual(classic);
  });
});

describe('isMapData: anyCardsPerSpace', () => {
  const map = (patch: Record<string, unknown>) => ({
    cities: [
      { id: 'a', name: 'A' },
      { id: 'b', name: 'B' },
    ],
    routes: [{ id: 'a_b', from: 'a', to: 'b', len: 9, color: 'Gray', double: false, ...patch }],
    tickets: [],
  });

  it('accetta un intero ≥ 2 o l\'assenza del campo', () => {
    expect(isMapData(map({}))).toBe(true);
    expect(isMapData(map({ anyCardsPerSpace: 4 }))).toBe(true);
  });

  it('rifiuta valori non interi o minori di 2', () => {
    for (const anyCardsPerSpace of [1, 0, -4, 2.5, '4', null]) {
      expect(isMapData(map({ anyCardsPerSpace })), `anyCardsPerSpace=${String(anyCardsPerSpace)}`).toBe(
        false,
      );
    }
  });
});
