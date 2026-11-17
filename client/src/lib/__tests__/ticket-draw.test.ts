import { describe, expect, it } from 'vitest';
import { canConfirmQuotas, defaultQuotas, moveQuota, quotasPayload } from '../ticket-draw';
import { passengerChoices, passengerPicks } from '../passengers';
import { modeBlurb, type RulesetInfo } from '../rulesets';
import type { TicketDrawMoves } from '../proto';

const draw: TicketDrawMoves = {
  total: 4,
  available: [
    { deck: 'regular', count: 3 },
    { deck: 'long', count: 10 },
  ],
};

describe('quote di pesca dei biglietti', () => {
  it('riempie i mazzi in ordine senza superare i rimasti', () => {
    expect(defaultQuotas(draw)).toEqual([3, 1]);
    expect(canConfirmQuotas(draw, [3, 1])).toBe(true);
  });

  it('rifiuta somme diverse dal totale o quote oltre i rimasti', () => {
    expect(canConfirmQuotas(draw, [2, 1])).toBe(false);
    expect(canConfirmQuotas(draw, [4, 0])).toBe(false);
    expect(canConfirmQuotas(draw, [0, 4])).toBe(true);
  });

  it('sposta un biglietto tra i mazzi solo se c’è posto', () => {
    expect(moveQuota(draw, [3, 1], 1, 0)).toEqual([3, 1]);
    expect(moveQuota(draw, [3, 1], 0, 1)).toEqual([2, 2]);
    expect(moveQuota(draw, [0, 4], 0, 1)).toEqual([0, 4]);
  });

  it('il payload omette i mazzi a zero', () => {
    expect(quotasPayload(draw, [0, 4])).toEqual([{ deck: 'long', count: 4 }]);
  });
});

describe('scelta dei passeggeri', () => {
  const onCities = [
    { city_id: 'berlin', passengers: [{ color: 'Blue', count: 2 }, { color: 'Red', count: 1 }] },
    { city_id: 'magdeburg', passengers: [{ color: 'Green', count: 1 }] },
  ];

  it('chiede la scelta solo negli estremi con più colori', () => {
    const choices = passengerChoices(onCities, { from: 'berlin', to: 'magdeburg' });
    expect(choices.map((c) => c.cityId)).toEqual(['berlin']);
    expect(passengerChoices(onCities, { from: 'magdeburg', to: 'leipzig' })).toEqual([]);
  });

  it('invia il colore scelto, o il primo se non è stato toccato nulla', () => {
    const choices = passengerChoices(onCities, { from: 'berlin', to: 'magdeburg' });
    expect(passengerPicks(choices, {})).toEqual([{ city_id: 'berlin', color: 'Blue' }]);
    expect(passengerPicks(choices, { berlin: 'Red' })).toEqual([{ city_id: 'berlin', color: 'Red' }]);
    // Un colore non più presente torna al primo disponibile.
    expect(passengerPicks(choices, { berlin: 'White' })).toEqual([{ city_id: 'berlin', color: 'Blue' }]);
    expect(passengerPicks([], {})).toBeUndefined();
  });
});

describe('modeBlurb delle regole nuove', () => {
  it('dice che la pesca è a scelta tra corti e lunghi e nomina passeggeri e carte Paese', () => {
    const base: RulesetInfo = {
      id: 'germany_2017', name: 'Germany', mode: 'Passeggeri', modeOrder: 1, mapId: 'germany',
      cities: 40, routes: 110, tickets: 89, minPlayers: 2, maxPlayers: 5,
      bonuses: [{ type: 'globetrotter', label: 'Globetrotter', points: 15 }], bigCities: [],
      initialDealt: 4, initialKeepMin: 2, drawCount: 4, drawKeepMin: 1,
      tunnels: false, ferries: false, stations: false,
      ticketDrawChoice: true, countryCards: false, passengers: true,
    };
    expect(modeBlurb(base)).toBe(
      '89 biglietti in gioco · ricevi 4 tra corti e lunghi, tieni 2 · pesca 4 tra corti e lunghi, tieni 1 · passeggeri · Globetrotter +15',
    );
    expect(modeBlurb({ ...base, ticketDrawChoice: false, passengers: false, countryCards: true, bonuses: [] })).toBe(
      '89 biglietti in gioco · ricevi 4, tieni 2 · pesca 4, tieni 1 · carte Paese · nessun bonus',
    );
  });
});
