// Chiusura di un dialog modale con un clic sull'overlay: conta solo se pressione e rilascio
// avvengono entrambi sull'overlay stesso, così trascinare dal dialog all'overlay (o viceversa)
// non chiude.

type OverlayEvent = Pick<Event, 'target' | 'currentTarget'> & { clientX?: number; clientY?: number };

/**
 * L'elemento sotto il punto di rilascio. Col tocco il browser cattura il puntatore sull'elemento
 * della pressione, quindi `target` non dice dove il dito si è sollevato: si guarda il punto.
 * Senza `elementFromPoint` (o senza coordinate) si ripiega su `target`.
 */
function releasedOnOverlay(e: OverlayEvent): boolean {
  if (
    typeof document !== 'undefined' &&
    typeof document.elementFromPoint === 'function' &&
    typeof e.clientX === 'number' &&
    typeof e.clientY === 'number'
  ) {
    return document.elementFromPoint(e.clientX, e.clientY) === e.currentTarget;
  }
  return e.target === e.currentTarget;
}

export interface OverlayDismiss {
  onpointerdown(e: OverlayEvent): void;
  onpointerup(e: OverlayEvent): void;
  onclick(e: OverlayEvent): void;
}

export function createOverlayDismiss(onDismiss: () => void): OverlayDismiss {
  let downOnOverlay = false;
  let upOnOverlay = false;
  return {
    onpointerdown(e) {
      downOnOverlay = e.target === e.currentTarget;
    },
    onpointerup(e) {
      upOnOverlay = releasedOnOverlay(e);
    },
    onclick(e) {
      const close = downOnOverlay && upOnOverlay && e.target === e.currentTarget;
      downOnOverlay = false;
      upOnOverlay = false;
      if (close) onDismiss();
    },
  };
}
