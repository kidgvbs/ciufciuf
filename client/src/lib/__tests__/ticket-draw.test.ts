import { describe, expect, it, vi } from 'vitest';
import { createDrawLatch } from '../ticket-draw';
import { passengerChoices, passengerPicks } from '../passengers';
import { modeBlurb, type RulesetInfo } from '../rulesets';

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
  it('nomina passeggeri e carte Paese', () => {
    const base: RulesetInfo = {
      id: 'germany_2017', name: 'Germany', mode: 'Passeggeri', modeOrder: 1, mapId: 'germany',
      cities: 40, routes: 110, tickets: 89, minPlayers: 2, maxPlayers: 5,
      bonuses: [{ type: 'globetrotter', label: 'Globetrotter', points: 15 }], bigCities: [],
      initialDealt: 5, initialKeepMin: 2, drawCount: 4, drawKeepMin: 1,
      tunnels: false, ferries: false, stations: false,
      countryCards: false, passengers: true,
    };
    expect(modeBlurb(base)).toBe(
      '89 biglietti in gioco · ricevi 5, tieni 2 · pesca 4, tieni 1 · passeggeri · Globetrotter +15',
    );
    expect(modeBlurb({ ...base, passengers: false, countryCards: true, bonuses: [] })).toBe(
      '89 biglietti in gioco · ricevi 5, tieni 2 · pesca 4, tieni 1 · carte Paese · nessun bonus',
    );
  });
});

describe('createDrawLatch', () => {
  it('due invii consecutivi producono un solo messaggio', () => {
    const changes: boolean[] = [];
    const latch = createDrawLatch((p) => changes.push(p));
    const send = vi.fn(() => true);
    expect(latch.submit(send)).toBe(true);
    expect(latch.submit(send)).toBe(false);
    expect(send).toHaveBeenCalledTimes(1);
    expect(changes).toEqual([true]);
  });

  it('un invio non partito non blocca, e dopo release si può reinviare', () => {
    const latch = createDrawLatch(() => {});
    const send = vi.fn().mockReturnValueOnce(false).mockReturnValue(true);
    expect(latch.submit(send)).toBe(false);
    expect(latch.pending).toBe(false);
    expect(latch.submit(send)).toBe(true);
    latch.release();
    expect(latch.submit(send)).toBe(true);
    expect(send).toHaveBeenCalledTimes(3);
  });

  it('un errore già visibile all’invio non rilascia, uno nuovo sì', () => {
    const stale = { code: 'OLD' };
    const latch = createDrawLatch(() => {});
    expect(latch.submit(() => true, stale)).toBe(true);
    latch.releaseOnError(stale);
    latch.releaseOnError(null);
    expect(latch.pending).toBe(true);
    latch.releaseOnError({ code: 'NEW' });
    expect(latch.pending).toBe(false);
  });
});
