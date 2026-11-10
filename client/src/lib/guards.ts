// Type guard condivisi per dati non fidati (WebSocket, localStorage).

/** Oggetto non nullo; i campi restano `unknown` e vanno verificati uno per uno. */
export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
