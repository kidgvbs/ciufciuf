// Presentazione dei salvataggi riprendibili nella lobby della TV.

/** Data e ora di un salvataggio (RFC 3339 dal server) nel formato it-IT; il testo grezzo se non è una data. */
export function formatSavedAt(iso: string, locale: string = 'it-IT'): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString(locale, {
    day: '2-digit',
    month: '2-digit',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}
