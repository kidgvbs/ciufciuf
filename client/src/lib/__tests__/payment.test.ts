import { describe, it, expect } from 'vitest';
import type { CardColor } from '../proto';
import {
  countCards,
  mostCommonColor,
  paymentCards,
  paymentCovered,
  payableRouteIds,
  routeColorToCard,
  routePayableWith,
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

describe('routeColorToCard', () => {
  it('traduce i colori del JSON e lascia le grigie senza colore', () => {
    expect(routeColorToCard('Red')).toBe('RED');
    expect(routeColorToCard('White')).toBe('WHITE');
    expect(routeColorToCard('Gray')).toBeNull();
  });
});

describe('routePayableWith: evidenziazione per carta selezionata', () => {
  // 3 rosse, 1 blu, 2 locomotive
  const hand: CardColor[] = ['RED', 'RED', 'RED', 'BLUE', 'LOCOMOTIVE', 'LOCOMOTIVE'];

  it('senza selezione non filtra nulla', () => {
    expect(routePayableWith({ len: 6, color: 'Gray' }, hand, null)).toBe(true);
  });

  it('colore scelto: rotta dello stesso colore o grigia, carte + locomotive >= lunghezza', () => {
    expect(routePayableWith({ len: 5, color: 'Red' }, hand, 'RED')).toBe(true); // 3 + 2 loco
    expect(routePayableWith({ len: 6, color: 'Red' }, hand, 'RED')).toBe(false); // 3 + 2 < 6
    expect(routePayableWith({ len: 3, color: 'Gray' }, hand, 'RED')).toBe(true);
    expect(routePayableWith({ len: 3, color: 'Blue' }, hand, 'RED')).toBe(false); // colore diverso
    expect(routePayableWith({ len: 3, color: 'Gray' }, hand, 'BLUE')).toBe(true); // 1 + 2 loco
    expect(routePayableWith({ len: 4, color: 'Gray' }, hand, 'BLUE')).toBe(false);
  });

  it('traghetti: servono le locomotive obbligatorie anche col colore scelto', () => {
    expect(routePayableWith({ len: 3, color: 'Red', ferries: 2 }, hand, 'RED')).toBe(true); // 1 + 2 loco
    expect(routePayableWith({ len: 3, color: 'Red', ferries: 3 }, hand, 'RED')).toBe(false); // solo 2 loco
    expect(routePayableWith({ len: 2, color: 'Gray', ferries: 1 }, ['RED', 'RED'], 'RED')).toBe(false);
  });

  it('locomotiva selezionata: solo le rotte pagabili con sole locomotive', () => {
    expect(routePayableWith({ len: 2, color: 'Blue' }, hand, 'LOCOMOTIVE')).toBe(true);
    expect(routePayableWith({ len: 2, color: 'Gray', ferries: 2 }, hand, 'LOCOMOTIVE')).toBe(true);
    expect(routePayableWith({ len: 3, color: 'Gray' }, hand, 'LOCOMOTIVE')).toBe(false);
  });

  it('payableRouteIds filtra le legali e scarta quelle sconosciute', () => {
    const routes = new Map([
      ['a', { len: 3, color: 'Red' }],
      ['b', { len: 3, color: 'Blue' }],
      ['c', { len: 2, color: 'Gray' }],
    ]);
    expect(payableRouteIds(['a', 'b', 'c', 'x'], routes, hand, 'RED')).toEqual(['a', 'c']);
    expect(payableRouteIds(['a', 'b', 'x'], routes, hand, null)).toEqual(['a', 'b', 'x']);
    expect(payableRouteIds(['a', 'b', 'c'], routes, hand, 'LOCOMOTIVE')).toEqual(['c']);
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
