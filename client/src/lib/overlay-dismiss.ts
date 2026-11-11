// Chiusura di un dialog modale con un clic sull'overlay: conta solo se la pressione è iniziata
// sull'overlay stesso, così trascinare dal dialog all'overlay non chiude.

type OverlayEvent = Pick<Event, 'target' | 'currentTarget'>;

export interface OverlayDismiss {
  onpointerdown(e: OverlayEvent): void;
  onclick(e: OverlayEvent): void;
}

export function createOverlayDismiss(onDismiss: () => void): OverlayDismiss {
  let downOnOverlay = false;
  return {
    onpointerdown(e) {
      downOnOverlay = e.target === e.currentTarget;
    },
    onclick(e) {
      const close = downOnOverlay && e.target === e.currentTarget;
      downOnOverlay = false;
      if (close) onDismiss();
    },
  };
}
