// Visibilità del dialog di scelta biglietti.
//
// Il dialog si apre quando compare un'offerta (o ne compare una con ID diversi) e "Annulla"
// lo tiene chiuso finché l'offerta non cambia: un nuovo PRIVATE_STATE con la stessa offerta
// non lo riapre.

/** Chiave stabile dell'offerta: vuota se non c'è nulla da scegliere. */
export function offerKey(offered: ReadonlyArray<{ id: string }>): string {
  return offered
    .map((t) => t.id)
    .sort()
    .join('|');
}

/** Il dialog è visibile se c'è un'offerta e non è stata chiusa con "Annulla". */
export function isDialogOpen(key: string, dismissedKey: string): boolean {
  return key !== '' && key !== dismissedKey;
}

/** `dismissedKey` aggiornato: quando l'offerta sparisce la chiusura manuale viene dimenticata. */
export function nextDismissedKey(key: string, dismissedKey: string): string {
  return key === '' ? '' : dismissedKey;
}
