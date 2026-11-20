import { describe, it, expect } from 'vitest';
import {
  findRoutePaymentWithFerry,
  maxFerryCards,
  payableRouteIds,
  routePaymentCards,
  routePaymentRules,
  routePaymentValid,
  rulesWithFerryCards,
  type RulesetPayment,
} from '../payment';
import { describeMove } from '../recent-moves';
import { regionsText } from '../scores';
import {
  ferryCardsConfig,
  hasRegions,
  isRulesetInfo,
  modeBlurb,
  regionsLabel,
  rulesetPayment,
  type RulesetInfo,
} from '../rulesets';
import type { CardColor, RecentMove } from '../proto';

const italyRules: RulesetPayment = { usableOn: null, ferryAnyCards: null, ferryCardWaves: 2 };
const regionsBonus = {
  type: 'regions',
  label: 'Regioni',
  points: 56,
  minRegions: 5,
  table: [1, 2, 4, 7, 11, 16, 22, 29, 37, 46, 56],
  doubleRegions: ['puglia', 'sardegna', 'sicilia'],
};
const italy: RulesetInfo = {
  id: 'italy',
  name: 'Italy',
  mapId: 'italy',
  mode: 'Standard',
  modeOrder: 0,
  cities: 39,
  routes: 128,
  tickets: 56,
  minPlayers: 2,
  maxPlayers: 5,
  bonuses: [regionsBonus],
  bigCities: [],
  initialDealt: 5,
  initialKeepMin: 3,
  drawCount: 4,
  drawKeepMin: 1,
  tunnels: false,
  ferries: true,
  stations: false,
  ferryCards: { count: 10, maxInHand: 2, wavesPerCard: 2 },
};

describe('pagamento con carte Traghetto', () => {
  // 4 spazi, 2 onde, grigia.
  const wave4 = { len: 4, color: 'Gray', ferries: 2 };
  const rules = routePaymentRules(wave4, italyRules);

  it('senza carte Traghetto nel ruleset le onde restano locomotive', () => {
    expect(routePaymentRules(wave4, null).ferryCardWaves).toBeUndefined();
    expect(maxFerryCards(routePaymentRules(wave4, null), 2)).toBe(0);
  });

  it('una carta copre due onde, il massimo è una per onda', () => {
    expect(rules.ferryCardWaves).toBe(2);
    expect(maxFerryCards(rules, 5)).toBe(2);
    expect(maxFerryCards(rules, 1)).toBe(1);
    expect(rulesWithFerryCards(rules, 1)).toMatchObject({ len: 2, ferries: 0 });
    expect(rulesWithFerryCards(rules, 2)).toMatchObject({ len: 2, ferries: 0 });
    expect(rulesWithFerryCards(rules, 0)).toMatchObject({ len: 4, ferries: 2 });
  });

  it('con 2 carte blu e una carta Traghetto la rotta è pagabile', () => {
    const hand: CardColor[] = ['BLUE', 'BLUE'];
    expect(routePaymentValid(rules, hand, 'BLUE', { locos: 0, groups: 0 })).toBe(false);
    const found = findRoutePaymentWithFerry(rules, hand, 'BLUE', 1);
    expect(found).toEqual({ locos: 0, groups: 0, ferryCards: 1 });
    expect(findRoutePaymentWithFerry(rules, hand, 'BLUE', 0)).toBeNull();
    const pay = rulesWithFerryCards(rules, 1);
    expect(routePaymentCards(pay, hand, 'BLUE', { locos: 0, groups: 0 })).toEqual(['BLUE', 'BLUE']);
  });

  it('usa il minimo di carte Traghetto: con le locomotive in mano non serve', () => {
    const hand: CardColor[] = ['BLUE', 'BLUE', 'LOCOMOTIVE', 'LOCOMOTIVE'];
    expect(findRoutePaymentWithFerry(rules, hand, 'BLUE', 2)?.ferryCards).toBe(0);
  });

  it('evidenzia le rotte pagabili solo grazie alle carte Traghetto', () => {
    const routes = new Map([['w', wave4]]);
    const hand: CardColor[] = ['BLUE', 'BLUE'];
    expect(payableRouteIds(['w'], routes, hand, 'BLUE', italyRules, 0)).toEqual([]);
    expect(payableRouteIds(['w'], routes, hand, 'BLUE', italyRules, 1)).toEqual(['w']);
  });

  it('il catalogo espone le onde per carta', () => {
    expect(rulesetPayment([italy], 'italy').ferryCardWaves).toBe(2);
    expect(rulesetPayment([], 'italy').ferryCardWaves).toBeUndefined();
  });
});

describe('catalogo Italy', () => {
  it('accetta ferryCards valide o null e rifiuta il resto', () => {
    expect(isRulesetInfo(italy)).toBe(true);
    expect(isRulesetInfo({ ...italy, ferryCards: null })).toBe(true);
    expect(isRulesetInfo({ ...italy, ferryCards: { count: 10, maxInHand: 0, wavesPerCard: 2 } })).toBe(false);
    expect(isRulesetInfo({ ...italy, ferryCards: { count: 10 } })).toBe(false);
    expect(isRulesetInfo({ ...italy, bonuses: [{ ...regionsBonus, minRegions: 0 }] })).toBe(false);
    expect(isRulesetInfo({ ...italy, bonuses: [{ ...regionsBonus, doubleRegions: [1] }] })).toBe(false);
  });

  it('ferryCardsConfig, hasRegions, regionsLabel e modeBlurb', () => {
    expect(ferryCardsConfig([italy], 'italy')).toEqual({ count: 10, maxInHand: 2, wavesPerCard: 2 });
    expect(ferryCardsConfig([{ ...italy, ferryCards: null }], 'italy')).toBeNull();
    expect(ferryCardsConfig([], 'italy')).toBeNull();
    expect(hasRegions([italy], 'italy')).toBe(true);
    expect(hasRegions([], 'italy')).toBe(false);
    expect(regionsLabel([italy], 'italy')).toBe('Regioni');
    expect(regionsLabel([], null)).toBe('Regioni');
    expect(modeBlurb(italy)).toBe(
      '56 biglietti in gioco · ricevi 5, tieni 3 · pesca 4, tieni 1 · Regioni fino a +56',
    );
  });
});

describe('testi Italy', () => {
  it('regionsText: punti e reti, null senza reti', () => {
    expect(regionsText(6, [7, 6])).toBe('+6 (7 + 6 Regioni)');
    expect(regionsText(0, [4])).toBe('0 (4 Regioni)');
    expect(regionsText(0, [])).toBeNull();
  });

  it('mossa recente: pesca carta Traghetto', () => {
    const move: RecentMove = { kind: 'DRAW_FERRY_CARD', player_id: 'a' };
    const line = describeMove(move, {
      players: [{ player_id: 'a', name: 'Anna', color: 'PINK' }],
      map: null,
    });
    expect(line.text).toBe('Anna pesca una carta Traghetto');
  });
});
