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

/** Vincoli della scelta dei biglietti (forma di `legal_moves.ticket_choice`). */
export interface TicketChoiceRules {
  keep_min: number;
  limits: ReadonlyArray<{ ticket_ids: ReadonlyArray<string>; max: number }>;
}

/** Un gruppo supera il suo massimo se i biglietti scelti che gli appartengono sono più di `max`. */
function exceedsLimit(selected: ReadonlySet<string>, limit: TicketChoiceRules['limits'][number]): boolean {
  return limit.ticket_ids.filter((id) => selected.has(id)).length > limit.max;
}

/** Conferma abilitata solo con vincoli noti: almeno `keep_min` scelti e nessun gruppo oltre il suo massimo. */
export function canConfirmTicketChoice(
  selected: ReadonlySet<string>,
  rules: TicketChoiceRules | null,
): boolean {
  if (!rules) return false;
  return selected.size >= rules.keep_min && !rules.limits.some((l) => exceedsLimit(selected, l));
}

/**
 * Testo di aiuto del dialog: «Tieni almeno N biglietti» più un «al massimo M biglietti lunghi»
 * per ogni limite (il gruppo è «lungo» se tutti i suoi biglietti sono del mazzo `long`).
 */
export function ticketChoiceHint(
  rules: TicketChoiceRules,
  deckOf: (ticketId: string) => string | undefined,
): string {
  const n = rules.keep_min;
  const parts = [`Tieni almeno ${n} ${n === 1 ? 'biglietto' : 'biglietti'}`];
  for (const l of rules.limits) {
    const long = l.ticket_ids.length > 0 && l.ticket_ids.every((id) => deckOf(id) === 'long');
    const noun = long ? (l.max === 1 ? 'biglietto lungo' : 'biglietti lunghi') : l.max === 1 ? 'biglietto di questo gruppo' : 'biglietti di questo gruppo';
    parts.push(`al massimo ${l.max} ${noun}`);
  }
  return parts.join(', ');
}
