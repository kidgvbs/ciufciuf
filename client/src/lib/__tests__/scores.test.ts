import { describe, it, expect } from 'vitest';
import { ticketResultLabel } from '../scores';
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
