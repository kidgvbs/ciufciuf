// Normalizzazione dei campi di `PublicState` che un server di versione diversa può omettere:
// la TV e i telefoni leggono sempre array veri, mai `undefined`.

import type { PublicState } from './proto';

export type NormalizedPublicState = Omit<
  PublicState,
  'stations' | 'recent_moves' | 'longest_path_route_ids' | 'tunnel_reveal'
> & {
  stations: NonNullable<PublicState['stations']>;
  recent_moves: NonNullable<PublicState['recent_moves']>;
  /** Vuoto fuori da GAME_OVER (nessun highlight del percorso più lungo). */
  longest_path_route_ids: string[];
  tunnel_reveal: PublicState['tunnel_reveal'];
};

export function normalizePublicState(ps: PublicState): NormalizedPublicState {
  return {
    ...ps,
    stations: ps.stations ?? [],
    recent_moves: ps.recent_moves ?? [],
    longest_path_route_ids: ps.longest_path_route_ids ?? [],
    tunnel_reveal: ps.tunnel_reveal ?? null,
    country_stacks: ps.country_stacks ?? [],
    passengers: ps.passengers ?? [],
    players_summary: ps.players_summary.map((p) => ({
      ...p,
      country_cards: p.country_cards ?? [],
      passengers: p.passengers ?? [],
    })),
  };
}
