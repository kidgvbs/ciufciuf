// Type guard condivisi per dati non fidati (WebSocket, localStorage).

/** Oggetto non nullo; i campi restano `unknown` e vanno verificati uno per uno. */
export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isArrayField(value: Record<string, unknown>, key: string): boolean {
  return Array.isArray(value[key]);
}

/** `true` se `value` ha tutti i campi di `PublicState` che gli store leggono. */
export function isPublicState(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value.game_id === 'string' &&
    typeof value.state === 'string' &&
    typeof value.deck_size === 'number' &&
    isArrayField(value, 'players_summary') &&
    isArrayField(value, 'claimed_routes') &&
    isArrayField(value, 'visible_deck')
  );
}

/** `true` se `value` ha tutti i campi di `PrivateState` che gli store leggono. */
export function isPrivateState(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value.seq === 'bigint' &&
    isArrayField(value, 'hand') &&
    isArrayField(value, 'tickets') &&
    isArrayField(value, 'offered_tickets')
  );
}

/** `true` se `value` è uno `Snapshot` con stato pubblico e (se presente) privato validi. */
export function isSnapshot(value: unknown): boolean {
  return (
    isRecord(value) &&
    isPublicState(value.public) &&
    (value.private === null || isPrivateState(value.private))
  );
}
