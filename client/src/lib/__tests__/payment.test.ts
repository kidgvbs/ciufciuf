import { describe, it, expect } from 'vitest';
import type { CardColor } from '../proto';
import {
  countCards,
  mostCommonColor,
  paymentCards,
  paymentCovered,
  suggestLocos,
  tunnelCardRaisesCost,
  tunnelPayColor,
} from '../payment';

const HAND: CardColor[] = ['RED', 'RED', 'BLUE', 'LOCOMOTIVE', 'LOCOMOTIVE', 'RED'];

describe('countCards / mostCommonColor', () => {
  it('conta le carte di un colore', () => {
    expect(countCards(HAND, 'RED')).toBe(3);
    expect(countCards(HAND, 'LOCOMOTIVE')).toBe(2);
    expect(countCards(HAND, 'GREEN')).toBe(0);
  });

  it('sceglie il colore non-locomotiva più presente', () => {
    expect(mostCommonColor(HAND)).toBe('RED');
    expect(mostCommonColor(['LOCOMOTIVE', 'LOCOMOTIVE'])).toBeNull();
    expect(mostCommonColor([])).toBeNull();
  });

  it('a parità prende il primo nell\'ordine dei colori', () => {
    expect(mostCommonColor(['BLUE', 'ORANGE'])).toBe('ORANGE');
  });
});

describe('suggestLocos', () => {
  it('rotta normale: solo le locomotive che servono a completare', () => {
    expect(suggestLocos({ total: 4, minLocos: 0, colorCount: 4, locoInHand: 2 })).toBe(0);
    expect(suggestLocos({ total: 4, minLocos: 0, colorCount: 3, locoInHand: 2 })).toBe(1);
    expect(suggestLocos({ total: 4, minLocos: 0, colorCount: 1, locoInHand: 2 })).toBe(2);
  });

  it('traghetto: mai meno delle locomotive obbligatorie', () => {
    expect(suggestLocos({ total: 4, minLocos: 2, colorCount: 4, locoInHand: 3 })).toBe(2);
    expect(suggestLocos({ total: 4, minLocos: 2, colorCount: 1, locoInHand: 3 })).toBe(3);
  });

  it('con meno locomotive in mano del necessario propone comunque le obbligatorie (la guardia le segnala)', () => {
    const spec = { total: 3, minLocos: 2, colorCount: 5, locoInHand: 1 };
    expect(suggestLocos(spec)).toBe(2);
    expect(paymentCovered(spec, 2)).toBe(false);
  });
});

describe('paymentCovered', () => {
  const spec = { total: 4, minLocos: 2, colorCount: 2, locoInHand: 3 };

  it('rispetta le locomotive obbligatorie del traghetto', () => {
    expect(paymentCovered(spec, 1)).toBe(false);
    expect(paymentCovered(spec, 2)).toBe(true);
    expect(paymentCovered(spec, 3)).toBe(true);
    expect(paymentCovered(spec, 4)).toBe(false); // più locomotive di quelle in mano
  });

  it('richiede carte colorate sufficienti per il resto', () => {
    expect(paymentCovered({ total: 4, minLocos: 0, colorCount: 2, locoInHand: 1 }, 1)).toBe(false);
    expect(paymentCovered({ total: 4, minLocos: 0, colorCount: 3, locoInHand: 1 }, 1)).toBe(true);
  });

  it('accetta il pagamento di sole locomotive senza colore', () => {
    expect(paymentCovered({ total: 2, minLocos: 0, colorCount: 0, locoInHand: 2 }, 2)).toBe(true);
    expect(paymentCovered({ total: 2, minLocos: 0, colorCount: 0, locoInHand: 1 }, 2)).toBe(false);
  });

  it('rifiuta locomotive oltre quelle in mano o oltre il totale', () => {
    expect(paymentCovered({ total: 2, minLocos: 0, colorCount: 5, locoInHand: 0 }, 1)).toBe(false);
    expect(paymentCovered({ total: 2, minLocos: 0, colorCount: 5, locoInHand: 5 }, 3)).toBe(false);
  });
});

describe('paymentCards', () => {
  it('prende le colorate e poi le locomotive dalla mano', () => {
    expect(paymentCards(HAND, 'RED', 3, 1)).toEqual(['RED', 'RED', 'LOCOMOTIVE']);
  });

  it('senza colore (solo locomotive) non prende carte colorate', () => {
    expect(paymentCards(HAND, null, 2, 2)).toEqual(['LOCOMOTIVE', 'LOCOMOTIVE']);
  });

  it('un colore LOCOMOTIVE non duplica le locomotive', () => {
    expect(paymentCards(HAND, 'LOCOMOTIVE', 2, 2)).toEqual(['LOCOMOTIVE', 'LOCOMOTIVE']);
  });
});

describe('tunnel: carte extra', () => {
  it('evidenzia le carte del colore del tunnel e le locomotive', () => {
    expect(tunnelCardRaisesCost('RED', 'RED')).toBe(true);
    expect(tunnelCardRaisesCost('LOCOMOTIVE', 'RED')).toBe(true);
    expect(tunnelCardRaisesCost('BLUE', 'RED')).toBe(false);
  });

  it('con tunnel LOCOMOTIVE conta solo le locomotive', () => {
    expect(tunnelCardRaisesCost('LOCOMOTIVE', 'LOCOMOTIVE')).toBe(true);
    expect(tunnelCardRaisesCost('RED', 'LOCOMOTIVE')).toBe(false);
  });

  it('il colore di pagamento è nullo se il tunnel è LOCOMOTIVE', () => {
    expect(tunnelPayColor('LOCOMOTIVE')).toBeNull();
    expect(tunnelPayColor('GREEN')).toBe('GREEN');
  });

  it('la scelta di carte extra: colore del tunnel + locomotive, esattamente extra_cost', () => {
    const extra = 2;
    const spec = { total: extra, minLocos: 0, colorCount: countCards(HAND, 'BLUE'), locoInHand: 2 };
    const locos = suggestLocos(spec);
    expect(locos).toBe(1);
    expect(paymentCovered(spec, locos)).toBe(true);
    expect(paymentCards(HAND, 'BLUE', extra, locos)).toEqual(['BLUE', 'LOCOMOTIVE']);
    // mano che non copre il costo extra
    expect(paymentCovered({ total: 3, minLocos: 0, colorCount: 1, locoInHand: 1 }, 1)).toBe(false);
  });
});
