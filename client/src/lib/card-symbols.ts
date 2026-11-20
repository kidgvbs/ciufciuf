// Simbolo di ogni colore carta, per chi non distingue i colori (come nel gioco
// ufficiale, ma con forme nostre). Lo stesso simbolo sta sulla carta e sugli
// spazi liberi delle rotte di quel colore. Forme piene in un riquadro 24×24,
// scelte per restare distinte anche a 10px e con un aggancio mnemonico.

import type { CardColor } from './proto';

/** Tre barre a 0°/60°/120° centrate in (12,12): un fiocco di neve a sei punte. */
function snowflake(): string {
  const half = 9.5;
  const w = 1.7;
  return [0, 60, 120]
    .map((deg) => {
      const a = (deg * Math.PI) / 180;
      const [cx, cy] = [Math.cos(a), Math.sin(a)];
      const [nx, ny] = [-cy, cx];
      const pts: Array<[number, number]> = [
        [half * cx + w * nx, half * cy + w * ny],
        [half * cx - w * nx, half * cy - w * ny],
        [-half * cx - w * nx, -half * cy - w * ny],
        [-half * cx + w * nx, -half * cy + w * ny],
      ];
      return `M${pts.map(([x, y]) => `${(12 + x).toFixed(2)} ${(12 + y).toFixed(2)}`).join('L')}Z`;
    })
    .join('');
}

/** Percorso SVG (riquadro 24×24) del simbolo di ogni colore; la locomotiva non ne ha: è già il jolly arcobaleno. */
export const CARD_SYMBOL_PATH: Record<Exclude<CardColor, 'LOCOMOTIVE'>, string> = {
  // Cuore
  RED: 'M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54z',
  // Cono stradale
  ORANGE: 'M12 2.5l6.4 16H5.6zM3 19.5h18V22H3z',
  // Fulmine
  YELLOW: 'M7 2v11h3v9l7-12h-4l4-8z',
  // Trifoglio
  GREEN: 'M7.6 7.6a4.4 4.4 0 1 0 8.8 0a4.4 4.4 0 1 0-8.8 0zM2.8 13.8a4.4 4.4 0 1 0 8.8 0a4.4 4.4 0 1 0-8.8 0zM12.4 13.8a4.4 4.4 0 1 0 8.8 0a4.4 4.4 0 1 0-8.8 0zM11 12h2l1.4 10h-4.8z',
  // Goccia
  BLUE: 'M12 2s-7 8-7 12.8a7 7 0 0 0 14 0C19 10 12 2 12 2z',
  // Corona
  PURPLE: 'M2.5 7l5 4.5L12 4l4.5 7.5 5-4.5-1.8 11H4.3zM4.3 19.5h15.4V22H4.3z',
  // Luna crescente
  BLACK: 'M12 3a9 9 0 1 0 9 9c0-.46-.04-.92-.1-1.36a5.389 5.389 0 0 1-4.4 2.26 5.403 5.403 0 0 1-3.14-9.8c-.44-.06-.9-.1-1.36-.1z',
  // Fiocco di neve
  WHITE: snowflake(),
};

/** Simbolo del colore, o `null` per la locomotiva. */
export function cardSymbolPath(color: CardColor): string | null {
  return color === 'LOCOMOTIVE' ? null : CARD_SYMBOL_PATH[color];
}
