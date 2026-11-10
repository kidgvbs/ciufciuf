// Numerazione delle azioni inviate al server (`client_seq`).
//
// Ogni valore è strettamente maggiore del precedente per lo stesso giocatore, anche dopo un
// reload della pagina o con l'orologio che torna indietro: il server scarta le azioni con
// `client_seq` <= ultimo visto, quindi un doppio invio non viene mai eseguito due volte.

/** Sottoinsieme di `Storage` usato per ricordare l'ultimo valore emesso. */
export interface SeqStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export interface ClientSeqSource {
  /** Prossimo `client_seq` per `playerId`. */
  next(playerId: string): bigint;
}

const KEY_PREFIX = 'ciufciuf-client-seq:';

function parseSeq(raw: string | null): bigint {
  if (!raw) return 0n;
  try {
    return BigInt(raw);
  } catch {
    return 0n;
  }
}

/**
 * Crea una sorgente di `client_seq`. Il valore parte da `now() * 1000` ed è persistito per
 * giocatore in `storage` (se disponibile); senza storage resta la memoria del processo.
 */
export function createClientSeqSource(
  getStorage: () => SeqStorage | null,
  now: () => number = Date.now,
): ClientSeqSource {
  const issued = new Map<string, bigint>();

  return {
    next(playerId: string): bigint {
      const key = KEY_PREFIX + playerId;
      let storage: SeqStorage | null = null;
      let stored = 0n;
      try {
        storage = getStorage();
        stored = parseSeq(storage?.getItem(key) ?? null);
      } catch (e) {
        console.warn('[client-seq] lettura fallita:', e);
      }
      const inMemory = issued.get(playerId) ?? 0n;
      const last = stored > inMemory ? stored : inMemory;
      const fromClock = BigInt(Math.trunc(now())) * 1000n;
      const value = fromClock > last ? fromClock : last + 1n;
      issued.set(playerId, value);
      try {
        storage?.setItem(key, value.toString());
      } catch (e) {
        console.warn('[client-seq] scrittura fallita:', e);
      }
      return value;
    },
  };
}

const defaultSource = createClientSeqSource(() => globalThis.sessionStorage ?? null);

/** Prossimo `client_seq` per `playerId`, persistito in `sessionStorage`. */
export function nextClientSeq(playerId: string): bigint {
  return defaultSource.next(playerId);
}
