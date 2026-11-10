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

/** Mappa PlayerColor → colore CSS per trenini/plance. */
export const PLAYER_COLOR_CSS: Record<PlayerColor, string> = {
  RED: '#c0392b',
  BLUE: '#2471a3',
  GREEN: '#1e8449',
  YELLOW: '#eab308',
  BLACK: '#2b2620',
};

