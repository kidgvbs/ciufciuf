// Pesca dei biglietti: un solo invio di `DRAW_TICKETS` per volta. Quanti biglietti si pescano e
// quanti se ne tengono lo decide il server (offerta + `legal_moves.ticket_choice`).

/**
 * Un solo invio di `DRAW_TICKETS` per volta: la carta «pesca biglietti» resta attiva
 * finché non arriva l'offerta, quindi un doppio tap invierebbe due richieste.
 * `release()` quando arriva l'offerta; `releaseOnError()` quando arriva un errore nuovo:
 * un errore già presente all'invio (banner non ancora chiuso) non è la risposta alla richiesta.
 */
export function createDrawLatch(onChange: (pending: boolean) => void) {
  let pending = false;
  let errorAtSubmit: unknown = null;
  const set = (value: boolean) => {
    if (pending === value) return;
    pending = value;
    onChange(value);
  };
  return {
    get pending() {
      return pending;
    },
    /**
     * Esegue `send` se non c'è un invio in corso; resta bloccato solo se è partito.
     * `currentError` è l'errore visibile all'invio, per distinguerlo da quelli successivi.
     */
    submit(send: () => boolean, currentError: unknown = null): boolean {
      if (pending) return false;
      const sent = send();
      if (sent) {
        errorAtSubmit = currentError;
        set(true);
      }
      return sent;
    },
    release() {
      set(false);
    },
    /** Rilascia solo per un errore diverso da quello presente all'invio; `null` non rilascia. */
    releaseOnError(error: unknown) {
      if (error !== null && error !== errorAtSubmit) set(false);
    },
  };
}
