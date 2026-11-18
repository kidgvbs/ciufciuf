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

/** Gruppo di biglietti offerti con un vincolo (forma di `legal_moves.ticket_choice.limits`). */
export interface TicketKeepLimit {
  ticket_ids: ReadonlyArray<string>;
  /** Almeno `min` biglietti del gruppo da tenere. */
  min: number;
  /** Al massimo `max` biglietti del gruppo da tenere (il numero del gruppo se non c'è un tetto). */
  max: number;
}

/** Vincoli della scelta dei biglietti (forma di `legal_moves.ticket_choice`). */
export interface TicketChoiceRules {
  keep_min: number;
  limits: ReadonlyArray<TicketKeepLimit>;
}

/** Conferma abilitata solo con vincoli noti: almeno `keep_min` scelti e ogni gruppo tra il suo minimo e il suo massimo. */
export function canConfirmTicketChoice(
  selected: ReadonlySet<string>,
  rules: TicketChoiceRules | null,
): boolean {
  if (!rules) return false;
  return (
    selected.size >= rules.keep_min &&
    rules.limits.every((l) => {
      const n = l.ticket_ids.filter((id) => selected.has(id)).length;
      return n >= l.min && n <= l.max;
    })
  );
}

/** «biglietto lungo», «biglietti corti»…: il gruppo è lungo/corto se tutti i suoi biglietti sono del mazzo `long`/`regular`. */
function groupNoun(
  limit: TicketKeepLimit,
  count: number,
  deckOf: (ticketId: string) => string | undefined,
): string {
  const one = count === 1;
  const all = (deck: string) =>
    limit.ticket_ids.length > 0 && limit.ticket_ids.every((id) => deckOf(id) === deck);
  if (all('long')) return one ? 'biglietto lungo' : 'biglietti lunghi';
  if (all('regular')) return one ? 'biglietto corto' : 'biglietti corti';
  return one ? 'biglietto di questo gruppo' : 'biglietti di questo gruppo';
}

/**
 * Testo di aiuto del dialog: «Tieni almeno N biglietti» più, per ogni gruppo, un «almeno M
 * biglietti corti» se ha un minimo e un «al massimo M biglietti lunghi» se ha un tetto.
 */
export function ticketChoiceHint(
  rules: TicketChoiceRules,
  deckOf: (ticketId: string) => string | undefined,
): string {
  const n = rules.keep_min;
  const parts = [`Tieni almeno ${n} ${n === 1 ? 'biglietto' : 'biglietti'}`];
  for (const l of rules.limits) {
    if (l.min > 0) parts.push(`almeno ${l.min} ${groupNoun(l, l.min, deckOf)}`);
    if (l.max < l.ticket_ids.length) parts.push(`al massimo ${l.max} ${groupNoun(l, l.max, deckOf)}`);
  }
  return parts.join(', ');
}

/**
 * Perché la selezione viola un gruppo («Tieni almeno 1 biglietto corto», «Tieni al massimo 1
 * biglietto lungo»), o `null` se tutti i gruppi sono rispettati.
 */
export function ticketChoiceProblem(
  selected: ReadonlySet<string>,
  rules: TicketChoiceRules | null,
  deckOf: (ticketId: string) => string | undefined,
): string | null {
  for (const l of rules?.limits ?? []) {
    const n = l.ticket_ids.filter((id) => selected.has(id)).length;
    if (n < l.min) return `Tieni almeno ${l.min} ${groupNoun(l, l.min, deckOf)}`;
    if (n > l.max) return `Tieni al massimo ${l.max} ${groupNoun(l, l.max, deckOf)}`;
  }
  return null;
}

/** Destinazione di un biglietto pronta da mostrare: nome della città e punti. */
export interface TicketRow {
  name: string;
  points: number;
}

/** Dati per `TicketCard`: con più destinazioni `rows` le elenca e `to` è vuoto. */
export interface TicketView {
  from: string;
  to: string;
  /** Punti del biglietto classico; per quello multi-destinazione il valore più alto. */
  points: number;
  rows: TicketRow[] | undefined;
}

/**
 * Cosa mostrare di un biglietto: le destinazioni arrivano dal server (`TicketInfo.destinations`);
 * se mancano si usa il descrittore della mappa. Senza nessuno dei due restano l'id e i punti noti.
 */
export function ticketView(
  info: {
    id: string;
    points: number;
    destinations?: ReadonlyArray<{ city_id: string; points: number }>;
  },
  desc: { from: string; to?: string; points?: number; destinations?: ReadonlyArray<{ to: string; points: number }> } | undefined,
  cityName: (id: string) => string,
): TicketView {
  const fromServer = (info.destinations ?? []).map((d) => ({ to: d.city_id, points: d.points }));
  const fromMap = desc?.destinations ?? (desc?.to !== undefined ? [{ to: desc.to, points: desc.points ?? info.points }] : []);
  const dests = (fromServer.length > 0 ? fromServer : fromMap).map((d) => ({
    name: cityName(d.to),
    points: d.points,
  }));
  const from = desc ? cityName(desc.from) : info.id;
  if (dests.length > 1) return { from, to: '', points: info.points, rows: dests };
  return { from, to: dests[0]?.name ?? '', points: dests[0]?.points ?? info.points, rows: undefined };
}
