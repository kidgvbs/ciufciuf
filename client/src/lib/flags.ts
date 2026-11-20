// Bandiere nazionali dei Paesi confinanti (disegni semplificati di pubblico dominio, fatti da noi
// con rettangoli e tracciati SVG: nessun asset esterno). Una voce per codice ISO 3166-1 alpha-2
// (maiuscolo, lo stesso del campo `flag` delle città `country` nei dati di mappa).
//
// I disegni vivono in una scatola 3:2 di 30×20 unità (`FLAG_W` × `FLAG_H`); chi li disegna li
// scala alla dimensione voluta. Le forme sono in ordine di pittura: le strisce usano il metodo
// del pittore (la successiva copre la precedente) così tra due strisce non resta nessun filo.

export interface FlagRect {
  kind: 'rect';
  x: number;
  y: number;
  w: number;
  h: number;
  fill: string;
}

export interface FlagPath {
  kind: 'path';
  d: string;
  fill: string;
  stroke?: string;
  strokeWidth?: number;
}

export type FlagShape = FlagRect | FlagPath;

/** Scatola di disegno delle bandiere (3:2). */
export const FLAG_W = 30;
export const FLAG_H = 20;

const WHITE = '#ffffff';

const rect = (x: number, y: number, w: number, h: number, fill: string): FlagRect => ({
  kind: 'rect',
  x,
  y,
  w,
  h,
  fill,
});

const path = (d: string, fill: string, stroke?: string, strokeWidth?: number): FlagPath => ({
  kind: 'path',
  d,
  fill,
  ...(stroke === undefined ? {} : { stroke, strokeWidth: strokeWidth ?? 1 }),
});

/** Strisce orizzontali di pari altezza, dall'alto in basso. */
function horizontal(...colors: string[]): FlagShape[] {
  return colors.map((fill, i) => rect(0, (FLAG_H * i) / colors.length, FLAG_W, FLAG_H - (FLAG_H * i) / colors.length, fill));
}

/** Strisce verticali di pari larghezza, dall'asta (sinistra) al battente. */
function vertical(...colors: string[]): FlagShape[] {
  return colors.map((fill, i) => rect((FLAG_W * i) / colors.length, 0, FLAG_W - (FLAG_W * i) / colors.length, FLAG_H, fill));
}

const FLAGS: Readonly<Record<string, readonly FlagShape[]>> = {
  // Germania: nero, rosso, oro.
  DE: horizontal('#000000', '#dd0000', '#ffce00'),
  // Austria: rosso, bianco, rosso.
  AT: horizontal('#ed2939', WHITE, '#ed2939'),
  // Francia: blu, bianco, rosso.
  FR: vertical('#0055a4', WHITE, '#ef4135'),
  // Italia: verde, bianco, rosso.
  IT: vertical('#009246', WHITE, '#ce2b37'),
  // Paesi Bassi: rosso, bianco, blu.
  NL: horizontal('#ae1c28', WHITE, '#21468b'),
  // Svizzera: campo rosso con croce bianca centrata.
  CH: [
    rect(0, 0, FLAG_W, FLAG_H, '#da291c'),
    rect(9, 8.2, 12, 3.6, WHITE),
    rect(13.2, 4, 3.6, 12, WHITE),
  ],
  // Danimarca: rosso con croce nordica bianca spostata verso l'asta.
  DK: [rect(0, 0, FLAG_W, FLAG_H, '#c8102e'), rect(9, 0, 4, FLAG_H, WHITE), rect(0, 8, FLAG_W, 4, WHITE)],
  // Russia: bianco, blu, rosso.
  RU: horizontal(WHITE, '#0039a6', '#d52b1e'),
  // Ucraina: blu sopra giallo.
  UA: horizontal('#0057b7', '#ffd700'),
  // Lituania: giallo, verde, rosso.
  LT: horizontal('#fdb913', '#006a44', '#c1272d'),
  // Cechia: bianco sopra rosso, triangolo blu dall'asta al centro.
  CZ: [
    rect(0, 0, FLAG_W, FLAG_H, WHITE),
    rect(0, FLAG_H / 2, FLAG_W, FLAG_H / 2, '#d7141a'),
    path(`M 0 0 L ${FLAG_W / 2} ${FLAG_H / 2} L 0 ${FLAG_H} Z`, '#11457e'),
  ],
  // Slovacchia: bianco, blu, rosso con scudo rosso (doppia croce bianca) verso l'asta.
  SK: [
    ...horizontal(WHITE, '#0b4ea2', '#ee1c25'),
    path('M 5 4.5 H 14 V 10.5 Q 14 15.5 9.5 17.5 Q 5 15.5 5 10.5 Z', '#ee1c25', WHITE, 1),
    rect(8.8, 6, 1.4, 8, WHITE),
    rect(7.4, 7.6, 4.2, 1.2, WHITE),
    rect(6.8, 10, 5.4, 1.2, WHITE),
  ],
  // Bielorussia: rosso sopra verde (2:1), striscia bianca all'asta con greca rossa a zig-zag.
  BY: [
    rect(0, 0, FLAG_W, FLAG_H, '#cf101a'),
    rect(0, (FLAG_H * 2) / 3, FLAG_W, FLAG_H / 3, '#007d2c'),
    rect(0, 0, 3.6, FLAG_H, WHITE),
    path('M 1.8 1.2 L 3.1 3.8 L 0.5 6.4 L 3.1 9 L 0.5 11.6 L 3.1 14.2 L 0.5 16.8 L 1.8 18.8', 'none', '#cf101a', 1.1),
  ],
  // Croazia: rosso, bianco, blu con scudo semplificato (scacchiera rosso-bianca 3×3) al centro.
  HR: [
    ...horizontal('#ff0000', WHITE, '#171796'),
    path('M 11.5 6 H 18.5 V 12 Q 18.5 14.5 15 15.5 Q 11.5 14.5 11.5 12 Z', WHITE),
    rect(11.5, 6, 7 / 3, 2, '#ff0000'),
    rect(11.5 + (7 * 2) / 3, 6, 7 / 3, 2, '#ff0000'),
    rect(11.5 + 7 / 3, 8, 7 / 3, 2, '#ff0000'),
    rect(11.5, 10, 7 / 3, 2, '#ff0000'),
    rect(11.5 + (7 * 2) / 3, 10, 7 / 3, 2, '#ff0000'),
    path('M 11.5 6 H 18.5 V 12 Q 18.5 14.5 15 15.5 Q 11.5 14.5 11.5 12 Z', 'none', '#ff0000', 0.6),
  ],
  // Monaco: rosso sopra bianco.
  MC: horizontal('#ce1126', WHITE),
};

/** Disegno della bandiera `code`, o `undefined` se il codice è assente o sconosciuto. */
export function flagShapes(code: string | undefined): readonly FlagShape[] | undefined {
  return code !== undefined && Object.hasOwn(FLAGS, code) ? FLAGS[code] : undefined;
}
