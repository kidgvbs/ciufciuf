// Palette colori per il rendering della mappa e dell'interfaccia.
// Tonalità calde "scatola da gioco", originali.

import type { CardColor, PlayerColor } from './proto';

/** Mappa CardColor → colore CSS per le carte/rotte. */
export const CARD_COLOR_CSS: Record<CardColor, string> = {
  RED: '#c0392b',
  ORANGE: '#d35400',
  YELLOW: '#eab308',
  GREEN: '#1e8449',
  BLUE: '#2471a3',
  PURPLE: '#7d3c98',
  BLACK: '#2b2620',
  WHITE: '#f4efe2',
  LOCOMOTIVE: '#8a7a68',
};

/** Nome italiano di ogni carta treno. */
export const CARD_COLOR_NAME: Record<CardColor, string> = {
  RED: 'Rossa',
  ORANGE: 'Arancio',
  YELLOW: 'Gialla',
  GREEN: 'Verde',
  BLUE: 'Blu',
  PURPLE: 'Viola',
  BLACK: 'Nera',
  WHITE: 'Bianca',
  LOCOMOTIVE: 'Jolly',
};

/** Mappa PlayerColor → colore CSS per trenini/plance (fuori dalla palette delle tratte). */
export const PLAYER_COLOR_CSS: Record<PlayerColor, string> = {
  PINK: '#ff5fa2',
  CYAN: '#22d3ee',
  LIME: '#a3e635',
  BROWN: '#8b5a2b',
  LAVENDER: '#c4b5fd',
  INDIGO: '#3f3cbb',
};

/** Nome italiano di ogni colore giocatore. */
export const PLAYER_COLOR_NAME: Record<PlayerColor, string> = {
  PINK: 'Rosa',
  CYAN: 'Ciano',
  LIME: 'Lime',
  BROWN: 'Marrone',
  LAVENDER: 'Lavanda',
  INDIGO: 'Indaco',
};

