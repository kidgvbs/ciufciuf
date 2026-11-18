import { describe, it, expect } from 'vitest';
import type { RecentMove } from '../proto';
import { describeMove, latestMoves, RECENT_MOVES_SHOWN, type MoveContext } from '../recent-moves';

const ctx: MoveContext = {
  players: [
    { player_id: 'p1', name: 'Gino', color: 'PINK' },
    { player_id: 'p2', name: 'Pia', color: 'CYAN' },
  ],
  map: {
    cities: [
      { id: 'brest', name: 'Brest' },
      { id: 'paris', name: 'Paris' },
      { id: 'madrid', name: 'Madrid' },
      { id: 'pamplona', name: 'Pamplona' },
    ],
    routes: [
      { id: 'r1', from: 'brest', to: 'paris' },
      { id: 'r2', from: 'madrid', to: 'pamplona' },
    ],
  },
};

describe('describeMove', () => {
  it('pesca carte: coperte e scoperte', () => {
    const m: RecentMove = { kind: 'DRAW_CARDS', player_id: 'p1', deck: 1, face_up: ['RED'] };
    const l = describeMove(m, ctx);
    expect(l.text).toBe('Gino pesca 2 carte (1 coperta + Rossa)');
    expect(l.name).toBe('Gino');
    expect(l.color).toBe('PINK');
  });

  it('pesca carte: due coperte, una sola carta scoperta, jolly', () => {
    expect(describeMove({ kind: 'DRAW_CARDS', player_id: 'p1', deck: 2, face_up: [] }, ctx).text)
      .toBe('Gino pesca 2 carte (2 coperte)');
    expect(describeMove({ kind: 'DRAW_CARDS', player_id: 'p1', deck: 0, face_up: ['LOCOMOTIVE'] }, ctx).text)
      .toBe('Gino pesca 1 carta (Jolly)');
  });

  it('biglietti: scelta in corso e scelta fatta', () => {
    expect(describeMove({ kind: 'DRAW_TICKETS', player_id: 'p2', kept: null }, ctx).text)
      .toBe('Pia sceglie i biglietti…');
    expect(describeMove({ kind: 'DRAW_TICKETS', player_id: 'p2', kept: 2 }, ctx).text)
      .toBe('Pia prende 2 biglietti');
    expect(describeMove({ kind: 'DRAW_TICKETS', player_id: 'p2', kept: 1 }, ctx).text)
      .toBe('Pia prende 1 biglietto');
  });

  it('mossa di tipo sconosciuto: riga neutra, nessun errore', () => {
    const m = { kind: 'FOO', player_id: 'p1' } as unknown as RecentMove;
    const l = describeMove(m, ctx);
    expect(typeof l.text).toBe('string');
    expect(l.text).toBe('Gino ha fatto una mossa');
    expect(l.name).toBe('Gino');
  });

  it('tunnel senza `revealed` non lancia errori', () => {
    const m = {
      kind: 'TUNNEL', player_id: 'p2', route_id: 'r2', color: 'RED', extra_cost: 0, outcome: 'CLAIMED',
    } as unknown as RecentMove;
    const l = describeMove(m, ctx);
    expect(l.cards).toEqual([]);
    expect(l.text).toBe('Pia: tunnel Madrid → Pamplona — nessuna carta rivelata · nessun extra → preso subito');
  });

  it('rotta presa', () => {
    expect(describeMove({ kind: 'CLAIM_ROUTE', player_id: 'p1', route_id: 'r1', points: 4 }, ctx).text)
      .toBe('Gino: Brest → Paris (+4)');
  });

  it('stazione', () => {
    expect(describeMove({ kind: 'BUILD_STATION', player_id: 'p1', city_id: 'paris' }, ctx).text)
      .toBe('Gino costruisce una stazione a Paris');
  });

  it.each([
    ['CLAIMED', 0, 'nessun extra → preso subito'],
    ['PENDING', 2, '+2 → in attesa'],
    ['PAID', 2, '+2 → pagato'],
    ['DECLINED', 2, '+2 → rinuncia'],
  ] as const)('tunnel %s', (outcome, extra, label) => {
    const l = describeMove(
      {
        kind: 'TUNNEL', player_id: 'p2', route_id: 'r2', revealed: ['RED', 'LOCOMOTIVE', 'BLUE'],
        color: 'RED', extra_cost: extra, outcome,
      },
      ctx,
    );
    expect(l.text).toBe(`Pia: tunnel Madrid → Pamplona — rivelate Rossa, Jolly, Blu · ${label}`);
    expect(l.suffix).toBe(` · ${label}`);
  });

  it('tunnel: evidenzia le carte che alzano il costo', () => {
    const l = describeMove(
      {
        kind: 'TUNNEL', player_id: 'p2', route_id: 'r2', revealed: ['RED', 'LOCOMOTIVE', 'BLUE'],
        color: 'RED', extra_cost: 2, outcome: 'PENDING',
      },
      ctx,
    );
    expect(l.cards).toEqual([
      { color: 'RED', raises: true },
      { color: 'LOCOMOTIVE', raises: true },
      { color: 'BLUE', raises: false },
    ]);
  });

  it('tunnel con colore LOCOMOTIVE: solo le locomotive alzano il costo', () => {
    const l = describeMove(
      {
        kind: 'TUNNEL', player_id: 'p1', route_id: 'r2', revealed: ['LOCOMOTIVE', 'RED'],
        color: 'LOCOMOTIVE', extra_cost: 1, outcome: 'PAID',
      },
      ctx,
    );
    expect(l.cards?.map((c) => c.raises)).toEqual([true, false]);
  });

  it('tunnel senza carte rivelate', () => {
    const l = describeMove(
      {
        kind: 'TUNNEL', player_id: 'p1', route_id: 'r2', revealed: [],
        color: 'RED', extra_cost: 0, outcome: 'CLAIMED',
      },
      ctx,
    );
    expect(l.cards).toEqual([]);
    expect(l.text).toBe('Gino: tunnel Madrid → Pamplona — nessuna carta rivelata · nessun extra → preso subito');
  });

  it('giocatore, città e rotta sconosciuti: si mostrano gli id', () => {
    const l = describeMove({ kind: 'CLAIM_ROUTE', player_id: 'zz', route_id: 'rx', points: 1 }, ctx);
    expect(l.text).toBe('zz: rx (+1)');
    expect(l.color).toBeNull();
    expect(describeMove({ kind: 'BUILD_STATION', player_id: 'zz', city_id: 'oslo' }, ctx).text)
      .toBe('zz costruisce una stazione a oslo');
  });

  it('mappa non ancora caricata: id al posto dei nomi', () => {
    const l = describeMove(
      { kind: 'CLAIM_ROUTE', player_id: 'p1', route_id: 'r1', points: 4 },
      { ...ctx, map: null },
    );
    expect(l.text).toBe('Gino: r1 (+4)');
  });
});

describe('latestMoves', () => {
  const moves: RecentMove[] = Array.from({ length: 8 }, (_, i) => ({
    kind: 'CLAIM_ROUTE', player_id: 'p1', route_id: `r${i}`, points: i,
  }));

  it('più recente per prima, al massimo RECENT_MOVES_SHOWN', () => {
    const out = latestMoves(moves);
    expect(out).toHaveLength(RECENT_MOVES_SHOWN);
    expect(out.map((m) => (m.kind === 'CLAIM_ROUTE' ? m.route_id : ''))).toEqual(['r7', 'r6', 'r5', 'r4', 'r3']);
  });

  it('con meno mosse del limite le mostra tutte e non modifica l\'input', () => {
    const few = moves.slice(0, 2);
    expect(latestMoves(few)).toHaveLength(2);
    expect(few[0]).toBe(moves[0]);
  });
});
