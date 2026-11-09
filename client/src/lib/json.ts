// JSON utilities: gestione bigint per i campi u64 del protocollo.
// ts-rs mappa `u64` → `bigint` in TypeScript, ma `JSON.parse` produce `number`.
// Usiamo un reviver/replacer per convertire in modo trasparente.

/** Chiavi note che contengono `u64` (seq / client_seq). */
const BIGINT_KEYS = new Set(['seq', 'client_seq']);

/**
 * `JSON.parse` con reviver che converte i `number` in `bigint`
 * per le chiavi `seq` e `client_seq`.
 */
export function parseWithBigInt(text: string): unknown {
  return JSON.parse(text, (_key: string, value: unknown): unknown => {
    if (typeof value === 'number' && BIGINT_KEYS.has(_key)) {
      return BigInt(value);
    }
    return value;
  });
}

/**
 * `JSON.stringify` con replacer che converte `bigint` in `number`.
 * Sicuro perché i nostri seq non superano 2^53 in una partita.
 */
export function stringifyWithBigInt(value: unknown): string {
  return JSON.stringify(value, (_key: string, value: unknown): unknown => {
    if (typeof value === 'bigint') {
      const n = Number(value);
      if (!Number.isSafeInteger(n)) {
        console.warn('bigint troppo grande per Number:', value);
      }
      return n;
    }
    return value;
  });
}