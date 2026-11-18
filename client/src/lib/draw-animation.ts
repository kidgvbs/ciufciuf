// Animazione "pesca": la carta entra in mano con ease-out dalla sorgente (mazzo o carta scoperta).
//
// Nessuna regola di gioco: la mano arriva dal server e qui si guarda solo il diff dei conteggi per
// colore (`addedColors`). La posizione della sorgente si registra al tap sull'azione di pesca; se
// la mano cresce entro `DRAW_SOURCE_TTL_MS` la carta nuova vola da lì alla sua posizione (FLIP con
// un clone fisso che viaggia sopra la pagina, così nessun contenitore lo ritaglia).
// Con `prefers-reduced-motion: reduce` non c'è alcuna animazione.

import type { CardColor } from './proto';

/** Durata del volo della carta. */
export const DRAW_ANIMATION_MS = 450;

/** Oltre questo tempo dal tap la sorgente è considerata stantia (azione rifiutata o mano cambiata per altro). */
export const DRAW_SOURCE_TTL_MS = 5000;

/** Rettangolo in coordinate viewport (copia di `DOMRect`, che non resta valido nel tempo). */
export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** Sorgente di una pesca: dove si è toccato e quando. */
export interface DrawSource {
  rect: Rect;
  at: number;
}

/** Conteggio delle carte della mano per colore. */
export function countByColor(hand: readonly CardColor[]): Map<CardColor, number> {
  const counts = new Map<CardColor, number>();
  for (const c of hand) counts.set(c, (counts.get(c) ?? 0) + 1);
  return counts;
}

/** Colori il cui conteggio è aumentato da `prev` a `next`, nell'ordine di `next`. */
export function addedColors(
  prev: ReadonlyMap<CardColor, number>,
  next: ReadonlyMap<CardColor, number>,
): CardColor[] {
  return [...next].filter(([color, n]) => n > (prev.get(color) ?? 0)).map(([color]) => color);
}

/** Colori il cui conteggio è diminuito da `prev` a `next` (con quante carte in meno), nell'ordine di `prev`. */
export function removedColors(
  prev: ReadonlyMap<CardColor, number>,
  next: ReadonlyMap<CardColor, number>,
): [CardColor, number][] {
  const out: [CardColor, number][] = [];
  for (const [color, n] of prev) {
    const left = next.get(color) ?? 0;
    if (n > left) out.push([color, n - left]);
  }
  return out;
}

/** `true` se la sorgente registrata a `source.at` è ancora valida a `now`. */
export function isFreshDrawSource<T extends { at: number }>(source: T, now: number): boolean {
  const age = now - source.at;
  return age >= 0 && age <= DRAW_SOURCE_TTL_MS;
}

export function prefersReducedMotion(): boolean {
  return typeof window !== 'undefined' && typeof window.matchMedia === 'function'
    && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/** Rettangolo di `el` adesso (da registrare al tap, prima che l'azione cambi il DOM). */
export function rectOf(el: Element): Rect {
  const r = el.getBoundingClientRect();
  return { left: r.left, top: r.top, width: r.width, height: r.height };
}

/**
 * Fa volare la carta `card` (bottone del ventaglio) da `from` alla sua posizione, con ease-out.
 * Un clone fisso compie il volo; l'originale resta invisibile finché il clone non arriva.
 * Il clone vive in `document.body` dentro una catena di wrapper `display: contents` che ripetono le
 * classi degli antenati di `card` fino a `scope` incluso: così i selettori discendenti (es.
 * `.hand-dock .train-card` in landscape) valgono anche per il clone e a fine volo coincide con la
 * carta reale.
 */
export function flyCard(from: Rect, card: HTMLElement, scope: HTMLElement): void {
  if (prefersReducedMotion() || typeof card.animate !== 'function') return;
  const to = rectOf(card);
  const w = card.offsetWidth;
  const h = card.offsetHeight;
  if (w === 0 || h === 0) return;

  // Centro del rettangolo ruotato = centro del suo bounding box: il clone, ruotato attorno al
  // proprio centro, coincide con la carta del ventaglio a fine volo.
  const cx = to.left + to.width / 2;
  const cy = to.top + to.height / 2;
  const dx = from.left + from.width / 2 - cx;
  const dy = from.top + from.height / 2 - cy;
  const startScale = Math.min(1, Math.max(0.5, from.height / h));
  const rot = Number.parseFloat(card.style.getPropertyValue('--rot')) || 0;

  const { root, ghost } = buildGhost(card, scope, cx, cy, w, h);
  document.body.appendChild(root);

  const flight = ghost.animate(
    [
      { transform: `translate(${dx}px, ${dy}px) rotate(0deg) scale(${startScale})`, opacity: 0.85 },
      { transform: `translate(0px, 0px) rotate(${rot}deg) scale(1)`, opacity: 1 },
    ],
    { duration: DRAW_ANIMATION_MS, easing: 'ease-out', fill: 'forwards' },
  );
  const hideOriginal = card.animate(
    [{ visibility: 'hidden' }, { visibility: 'hidden' }],
    { duration: DRAW_ANIMATION_MS },
  );
  const done = () => {
    root.remove();
    hideOriginal.cancel();
  };
  flight.addEventListener('finish', done, { once: true });
  flight.addEventListener('cancel', done, { once: true });
}

/**
 * Clone fisso di `card` centrato in (`cx`, `cy`) dentro la catena di wrapper `display: contents`
 * (classi degli antenati di `card` fino a `scope` incluso). Non è ancora nel DOM: lo monta chi lo usa.
 */
function buildGhost(
  card: HTMLElement,
  scope: HTMLElement,
  cx: number,
  cy: number,
  w: number,
  h: number,
): { root: HTMLElement; ghost: HTMLElement } {
  const ghost = card.cloneNode(true) as HTMLElement;
  ghost.classList.remove('focused');
  ghost.removeAttribute('aria-label');
  ghost.setAttribute('aria-hidden', 'true');
  ghost.tabIndex = -1;
  ghost.inert = true;
  Object.assign(ghost.style, {
    position: 'fixed',
    left: `${cx - w / 2}px`,
    top: `${cy - h / 2}px`,
    width: `${w}px`,
    height: `${h}px`,
    margin: '0',
    zIndex: '1000',
    pointerEvents: 'none',
    transition: 'none',
    transformOrigin: 'center',
  });
  const root = document.createElement('div');
  root.style.display = 'contents';
  root.setAttribute('aria-hidden', 'true');
  let parent: HTMLElement = root;
  const chain: HTMLElement[] = [];
  for (let el = card.parentElement; el && el !== document.body; el = el.parentElement) {
    const wrapper = document.createElement('div');
    wrapper.className = el.className;
    wrapper.style.display = 'contents';
    chain.unshift(wrapper);
    if (el === scope) break;
  }
  for (const wrapper of chain) parent = parent.appendChild(wrapper);
  parent.appendChild(ghost);
  return { root, ghost };
}

// ── Scarto: le carte spese per una rotta rimpiccioliscono verso il mazzo ─────────────────────

/** Durata del rimpicciolimento di una carta scartata. */
export const DISCARD_ANIMATION_MS = 400;

/** Sfasamento fra una carta scartata e la successiva. */
export const DISCARD_STAGGER_MS = 60;

/** Tetto di copie animate per colore (con 6 carte uguali bastano poche per rendere l'idea). */
const MAX_DISCARD_COPIES = 4;

/** Carta del ventaglio fotografata al tap: dopo l'invio può sparire dal DOM (conteggio a 0). */
export interface DiscardGhost {
  color: CardColor;
  /** Centro della carta al tap, coordinate viewport. */
  cx: number;
  cy: number;
  /** Rotazione del ventaglio (gradi). */
  rot: number;
  /** Clone già dentro la catena di wrapper, non montato. */
  root: HTMLElement;
}

/** Carte da scartare fotografate al tap, destinazione (mazzo) e istante. */
export interface DiscardSource {
  ghosts: DiscardGhost[];
  target: Rect;
  at: number;
}

/**
 * Fotografa, PRIMA di inviare l'azione, la carta del ventaglio di ogni colore in `colors` e il
 * rettangolo del mazzo. `null` se non c'è nulla da animare (reduced motion, niente `animate`,
 * elementi mancanti).
 */
export function captureDiscardSource(
  colors: readonly CardColor[],
  fan: HTMLElement | undefined,
  deck: Element | null | undefined,
  scope: HTMLElement | undefined,
  now: number,
): DiscardSource | null {
  if (prefersReducedMotion() || !fan || !deck || !scope) return null;
  const ghosts: DiscardGhost[] = [];
  for (const color of new Set(colors)) {
    const card = fan.querySelector<HTMLElement>(`[data-card-color="${color}"]`);
    if (!card || typeof card.animate !== 'function') continue;
    const w = card.offsetWidth;
    const h = card.offsetHeight;
    if (w === 0 || h === 0) continue;
    const r = rectOf(card);
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const rot = Number.parseFloat(card.style.getPropertyValue('--rot')) || 0;
    ghosts.push({ color, cx, cy, rot, root: buildGhost(card, scope, cx, cy, w, h).root });
  }
  if (ghosts.length === 0) return null;
  return { ghosts, target: rectOf(deck), at: now };
}

/**
 * Anima verso il mazzo le carte dei colori in `removed` (diff della mano): ogni carta
 * rimpicciolisce e svanisce con ease-in, a scalini. I cloni si rimuovono a fine/annullo.
 */
export function discardCards(
  source: DiscardSource,
  removed: readonly (readonly [CardColor, number])[],
): void {
  if (prefersReducedMotion()) return;
  const tx = source.target.left + source.target.width / 2;
  const ty = source.target.top + source.target.height / 2;
  let index = 0;
  for (const [color, count] of removed) {
    const g = source.ghosts.find((x) => x.color === color);
    if (!g) continue;
    for (let i = 0; i < Math.min(count, MAX_DISCARD_COPIES); i++) {
      const root = g.root.cloneNode(true) as HTMLElement;
      const ghost = root.querySelector<HTMLElement>('[data-card-color]');
      if (!ghost || typeof ghost.animate !== 'function') continue;
      document.body.appendChild(root);
      const done = () => root.remove();
      const flight = ghost.animate(
        [
          { transform: `translate(0px, 0px) rotate(${g.rot}deg) scale(1)`, opacity: 1 },
          { transform: `translate(${tx - g.cx}px, ${ty - g.cy}px) rotate(0deg) scale(0.2)`, opacity: 0 },
        ],
        {
          duration: DISCARD_ANIMATION_MS,
          delay: index++ * DISCARD_STAGGER_MS,
          easing: 'ease-in',
          fill: 'both',
        },
      );
      flight.addEventListener('finish', done, { once: true });
      flight.addEventListener('cancel', done, { once: true });
    }
  }
}
