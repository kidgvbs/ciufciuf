// Dedup idempotente lato client per SNAPSHOT/risposte usando `client_seq` (E6).
// Il server droppa azioni con `client_seq <= last`, ma il client deve anche
// scartare messaggi outbound duplicati (es. PRIVATE_STATE già visto).

import type { PrivateState } from './proto';

/**
 * Filtro anti-duplicati per i messaggi inbound.
 * Scarta `PRIVATE_STATE` e `SNAPSHOT` con `private.seq <= ultimo visto`.
 *
 * Restituisce `true` se il messaggio è nuovo e va processato.
 */
export function isPrivateStateFresh(
  lastSeq: bigint,
  privateState: PrivateState,
): boolean {
  return privateState.seq > lastSeq;
}

/**
 * Verifica se uno `SNAPSHOT` privato è più recente di quello già visto.
 */
export function isSnapshotPrivateFresh(
  lastSeq: bigint,
  privateState: PrivateState | null,
): boolean {
  if (!privateState) return true; // spettatore: sempre nuovo
  return privateState.seq > lastSeq;
}