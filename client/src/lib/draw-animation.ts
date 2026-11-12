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

/** `true` se la sorgente registrata a `source.at` è ancora valida a `now`. */
export function isFreshDrawSource(source: DrawSource, now: number): boolean {
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
