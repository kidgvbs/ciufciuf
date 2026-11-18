import { describe, it, expect } from 'vitest';
import type { CardColor } from '../proto';
import {
  countCards,
  findRoutePayment,
  locoBounds,
  maxGroups,
  mostCommonColor,
  paymentCards,
  paymentCovered,
  payableRouteIds,
  routeColorToCard,
  routePayableWith,
  routePaymentCards,
  routePaymentRules,
  routePaymentValid,
  suggestLocos,
  suggestRoutePayment,
  tunnelCardRaisesCost,
  tunnelPayColor,
  type RulesetPayment,
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

// Regole dati dal ruleset (Nordic / Switzerland): il client le legge dal catalogo, il server arbitra.
const NORDIC: RulesetPayment = { usableOn: ['tunnel', 'ferry'], ferryAnyCards: 3 };
const SWISS: RulesetPayment = { usableOn: ['tunnel'], ferryAnyCards: null };
const cards = (parts: [CardColor, number][]): CardColor[] =>
  parts.flatMap(([c, n]) => Array.from({ length: n }, () => c));

describe('routePaymentRules', () => {
  it('senza ruleset: locomotive ovunque, nessun gruppo (regole classiche)', () => {
    expect(routePaymentRules({ len: 3, color: 'Red' })).toEqual({
      len: 3, ferries: 0, singleLoco: true, groupSize: null,
    });
    expect(routePaymentRules({ len: 3, color: 'Red', ferries: 1 }, { usableOn: null, ferryAnyCards: null }))
      .toMatchObject({ ferries: 1, singleLoco: true, groupSize: null });
  });

  it('usableOn: la locomotiva singola solo su tunnel e/o traghetti', () => {
    const plain = { len: 3, color: 'Red' };
    expect(routePaymentRules(plain, NORDIC).singleLoco).toBe(false);
    expect(routePaymentRules({ ...plain, tunnel: true }, NORDIC).singleLoco).toBe(true);
    expect(routePaymentRules({ ...plain, ferries: 1 }, NORDIC).singleLoco).toBe(true);
    expect(routePaymentRules(plain, SWISS).singleLoco).toBe(false);
    expect(routePaymentRules({ ...plain, tunnel: true }, SWISS).singleLoco).toBe(true);
    expect(routePaymentRules({ ...plain, ferries: 1 }, SWISS).singleLoco).toBe(false);
  });

  it('gruppi: 3 carte per locomotiva solo sui traghetti, anyCardsPerSpace sulla rotta', () => {
    expect(routePaymentRules({ len: 3, color: 'Red' }, NORDIC).groupSize).toBeNull();
    expect(routePaymentRules({ len: 3, color: 'Red', ferries: 1 }, NORDIC).groupSize).toBe(3);
    expect(routePaymentRules({ len: 9, color: 'Gray', anyCardsPerSpace: 4 }, NORDIC).groupSize).toBe(4);
  });
});

describe('routePaymentValid / findRoutePayment: Nordic', () => {
  const plain = routePaymentRules({ len: 3, color: 'Red' }, NORDIC);
  const ferry = routePaymentRules({ len: 3, color: 'Blue', ferries: 1 }, NORDIC);
  const murmansk = routePaymentRules({ len: 9, color: 'Gray', anyCardsPerSpace: 4 }, NORDIC);

  it('rotta normale: la locomotiva è rifiutata, anche se la mano la avrebbe', () => {
    const hand = cards([['RED', 2], ['LOCOMOTIVE', 1]]);
    expect(findRoutePayment(plain, hand, 'RED')).toBeNull();
    expect(findRoutePayment(plain, cards([['RED', 3], ['LOCOMOTIVE', 1]]), 'RED')).toEqual({ locos: 0, groups: 0 });
    expect(findRoutePayment(plain, cards([['LOCOMOTIVE', 3]]), null)).toBeNull();
  });

  it('traghetto: 3 carte qualsiasi al posto della locomotiva', () => {
    expect(findRoutePayment(ferry, cards([['BLUE', 2], ['LOCOMOTIVE', 1]]), 'BLUE')).toEqual({ locos: 1, groups: 0 });
    const hand = cards([['BLUE', 2], ['RED', 2], ['GREEN', 1]]);
    expect(findRoutePayment(ferry, hand, 'BLUE')).toEqual({ locos: 0, groups: 1 });
    expect(routePaymentValid(ferry, hand, 'BLUE', { locos: 0, groups: 1 })).toBe(true);
    // 2 carte blu + 2 qualsiasi: il gruppo non si completa.
    expect(findRoutePayment(ferry, cards([['BLUE', 2], ['RED', 2]]), 'BLUE')).toBeNull();
    expect(findRoutePayment(ferry, cards([['BLUE', 4]]), 'BLUE')).toBeNull();
  });

  it('traghetto grigio: un colore per le carte singole, più i gruppi', () => {
    const gray = routePaymentRules({ len: 3, color: 'Gray', ferries: 1 }, NORDIC);
    const hand = cards([['BLUE', 1], ['LOCOMOTIVE', 1], ['RED', 2], ['GREEN', 1]]);
    expect(findRoutePayment(gray, hand, 'BLUE')).toEqual({ locos: 1, groups: 1 });
    expect(routePayableWith({ len: 3, color: 'Gray', ferries: 1 }, hand, 'BLUE', NORDIC)).toBe(true);
  });

  it('Murmansk–Lieksa: 7 verdi + 8 carte qualsiasi (locomotive comprese)', () => {
    const hand = cards([['GREEN', 7], ['RED', 3], ['LOCOMOTIVE', 3], ['BLUE', 2]]);
    expect(findRoutePayment(murmansk, hand, 'GREEN')).toEqual({ locos: 0, groups: 2 });
    const payment = suggestRoutePayment(murmansk, hand, 'GREEN');
    const sent = routePaymentCards(murmansk, hand, 'GREEN', payment);
    expect(sent).toHaveLength(15);
    expect(countCards(sent, 'GREEN')).toBeGreaterThanOrEqual(7);
    // 7 verdi singole + 8 carte qualsiasi: sono tutte le 15 della mano.
    expect(countCards(sent, 'LOCOMOTIVE')).toBe(3);
    // Con una mano più ricca le locomotive sono l'ultima scelta per riempire i gruppi.
    const rich = [...hand, ...cards([['RED', 4]])];
    expect(countCards(routePaymentCards(murmansk, rich, 'GREEN', { locos: 0, groups: 2 }), 'LOCOMOTIVE')).toBe(0);
    // 8 locomotive: due gruppi da 4 non bastano per 9 spazi.
    expect(findRoutePayment(murmansk, cards([['LOCOMOTIVE', 8]]), null)).toBeNull();
    expect(findRoutePayment(murmansk, cards([['LOCOMOTIVE', 36]]), null)).toEqual({ locos: 0, groups: 9 });
  });

  it('carte avanzate: il pagamento composto è sempre esatto', () => {
    const hand = cards([['GREEN', 7], ['RED', 7]]);
    expect(findRoutePayment(murmansk, hand, 'GREEN')).toBeNull();
    const ok = cards([['GREEN', 8], ['RED', 4]]);
    const sent = routePaymentCards(murmansk, ok, 'GREEN', { locos: 0, groups: 1 });
    expect(sent).toHaveLength(12);
  });
});

describe('routePaymentValid: Switzerland', () => {
  it('locomotiva solo sui tunnel', () => {
    const hand = cards([['RED', 1], ['LOCOMOTIVE', 1]]);
    const road = { len: 2, color: 'Red' };
    expect(findRoutePayment(routePaymentRules(road, SWISS), hand, 'RED')).toBeNull();
    expect(findRoutePayment(routePaymentRules({ ...road, tunnel: true }, SWISS), hand, 'RED')).toEqual({
      locos: 1, groups: 0,
    });
    expect(routePayableWith(road, hand, 'RED', SWISS)).toBe(false);
    expect(routePayableWith({ ...road, tunnel: true }, hand, 'RED', SWISS)).toBe(true);
    expect(routePayableWith(road, hand, 'LOCOMOTIVE', SWISS)).toBe(false);
  });

  it('payableRouteIds filtra con le regole del ruleset', () => {
    const routes = new Map([
      ['road', { len: 2, color: 'Red' }],
      ['tunnel', { len: 2, color: 'Red', tunnel: true }],
    ]);
    const hand = cards([['RED', 1], ['LOCOMOTIVE', 1]]);
    expect(payableRouteIds(['road', 'tunnel'], routes, hand, 'RED', SWISS)).toEqual(['tunnel']);
    // Senza regole (ruleset classico) entrambe restano pagabili.
    expect(payableRouteIds(['road', 'tunnel'], routes, hand, 'RED')).toEqual(['road', 'tunnel']);
  });
});

describe('limiti dei selettori', () => {
  it('locoBounds: senza gruppi il traghetto fissa il minimo; senza loco ammesse il massimo è i simboli', () => {
    expect(locoBounds(routePaymentRules({ len: 3, color: 'Red', ferries: 2 }), 5)).toEqual({ min: 2, max: 3 });
    expect(locoBounds(routePaymentRules({ len: 3, color: 'Red' }, NORDIC), 5)).toEqual({ min: 0, max: 0 });
    expect(locoBounds(routePaymentRules({ len: 3, color: 'Red', ferries: 1 }, NORDIC), 5)).toEqual({ min: 0, max: 3 });
  });

  it('maxGroups: 0 senza sostituzioni, altrimenti le carte in mano divise per il gruppo', () => {
    const hand = cards([['RED', 9]]);
    expect(maxGroups(routePaymentRules({ len: 3, color: 'Red' }), hand)).toBe(0);
    expect(maxGroups(routePaymentRules({ len: 9, color: 'Gray', anyCardsPerSpace: 4 }), hand)).toBe(2);
  });

  it('comportamento classico invariato: suggestRoutePayment = locomotive minime', () => {
    const rules = routePaymentRules({ len: 5, color: 'Red' });
    expect(suggestRoutePayment(rules, HAND, 'RED')).toEqual({ locos: 2, groups: 0 });
    expect(routePaymentCards(rules, HAND, 'RED', { locos: 2, groups: 0 })).toEqual(
      paymentCards(HAND, 'RED', 5, 2),
    );
  });
});
