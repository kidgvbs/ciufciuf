import { describe, it, expect } from 'vitest';
import { largestNetworkWinners, ticketResultLabel } from '../scores';
import type { FinalScoreInfo, PlayerSummary } from '../proto';
import type { MapData } from '../map-utils';

const MAP: MapData = {
  cities: [
    { id: 'zh', name: 'Zürich' },
    { id: 'fr', name: 'Frankreich', country: true },
    { id: 'de', name: 'Deutschland', country: true },
    { id: 'it', name: 'Italien', country: true },
  ],
  routes: [],
  tickets: [
    { id: 'zh_fr', from: 'zh', to: 'fr', points: 6, deck: 'regular' },
    {
      id: 'fr_de_it',
      from: 'fr',
      destinations: [
        { to: 'de', points: 5 },
        { to: 'it', points: 13 },
      ],
      deck: 'regular',
    },
  ],
} as unknown as MapData;

describe('ticketResultLabel', () => {
  it('biglietto classico completato', () => {
    const r = { ticketId: 'zh_fr', completed: true, cityId: 'fr', points: 6, mandala: false };
    expect(ticketResultLabel(r, MAP)).toBe('✓ Zürich → Frankreich +6');
  });

  it('multi-destinazione completato: solo la destinazione realizzata e i suoi punti', () => {
    const r = { ticketId: 'fr_de_it', completed: true, cityId: 'it', points: 13, mandala: false };
    expect(ticketResultLabel(r, MAP)).toBe('✓ Frankreich → Italien +13');
  });

  it('multi-destinazione fallito: tutte le destinazioni e i punti persi', () => {
    const r = { ticketId: 'fr_de_it', completed: false, cityId: null, points: -5, mandala: false };
    expect(ticketResultLabel(r, MAP)).toBe('✗ Frankreich → Deutschland / Italien -5');
  });

  it('biglietto che conta per il Mandala: suffisso 🪷', () => {
    const r = { ticketId: 'zh_fr', completed: true, cityId: 'fr', points: 6, mandala: true };
    expect(ticketResultLabel(r, MAP)).toBe('✓ Zürich → Frankreich +6 🪷');
  });

  it('senza mappa restano gli id', () => {
    const r = { ticketId: 'zh_fr', completed: false, cityId: null, points: -6, mandala: false };
    expect(ticketResultLabel(r, null)).toBe('✗ zh_fr -6');
  });
});

describe('largestNetworkWinners', () => {
  const players = [
    { player_id: 'a', name: 'Anna' },
    { player_id: 'b', name: 'Luca' },
    { player_id: 'c', name: 'Mara' },
  ] as unknown as PlayerSummary[];
  const score = (playerId: string, largestNetworkBonus: number, largestNetwork: number) =>
    ({ playerId, largestNetworkBonus, largestNetwork }) as unknown as FinalScoreInfo;

  it('nome del vincitore e dimensione della rete', () => {
    const scores = [score('a', 10, 9), score('b', 0, 7), score('c', 0, 0)];
    expect(largestNetworkWinners(scores, players)).toBe('Anna (9 città)');
  });

  it('pari merito: tutti i nomi, una sola dimensione', () => {
    const scores = [score('a', 10, 8), score('b', 10, 8), score('c', 0, 3)];
    expect(largestNetworkWinners(scores, players)).toBe('Anna, Luca (8 città)');
  });

  it('nessun bonus assegnato: nessuna riga; giocatore ignoto: resta l\'id', () => {
    expect(largestNetworkWinners([score('a', 0, 0), score('b', 0, 0)], players)).toBeNull();
    expect(largestNetworkWinners([score('z', 10, 4)], players)).toBe('z (4 città)');
  });
});
