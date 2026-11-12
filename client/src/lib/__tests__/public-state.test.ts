import { describe, it, expect } from 'vitest';
import type { PublicState } from '../proto';
import { normalizePublicState } from '../public-state';
import { isPublicState } from '../guards';

function base(): Record<string, unknown> {
  return {
    game_id: 'g1',
    protocol_version: 1,
    state: 'IN_PROGRESS',
    deck_size: 10,
    players_summary: [],
    claimed_routes: [],
    visible_deck: [],
    current_turn_player_id: null,
    last_round: false,
    last_round_trigger_player_id: null,
    final_scores: null,
    longest_path_player_ids: null,
  };
}

describe('normalizePublicState', () => {
  it('un PUBLIC_STATE senza stations e recent_moves produce array vuoti', () => {
    const n = normalizePublicState(base() as unknown as PublicState);
    expect(n.stations).toEqual([]);
    expect(n.recent_moves).toEqual([]);
    expect(n.longest_path_route_ids).toEqual([]);
    expect(n.tunnel_reveal).toBeNull();
  });

  it('non altera i campi presenti', () => {
    const ps = {
      ...base(),
      stations: [{ city_id: 'paris', player_id: 'p1' }],
      recent_moves: [{ kind: 'BUILD_STATION', player_id: 'p1', city_id: 'paris' }],
      longest_path_route_ids: ['r1'],
    } as unknown as PublicState;
    const n = normalizePublicState(ps);
    expect(n.stations).toBe(ps.stations);
    expect(n.recent_moves).toBe(ps.recent_moves);
    expect(n.longest_path_route_ids).toBe(ps.longest_path_route_ids);
  });
});

describe('isPublicState', () => {
  it('accetta i campi Europe assenti (server vecchio) e rifiuta quelli non array', () => {
    expect(isPublicState(base())).toBe(true);
    expect(isPublicState({ ...base(), stations: [] })).toBe(true);
    expect(isPublicState({ ...base(), stations: 'x' })).toBe(false);
    expect(isPublicState({ ...base(), recent_moves: {} })).toBe(false);
  });
});
