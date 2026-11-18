// Regole speciali sul controller: «le ho già lette» per partita, così un reload non riapre il modale.
//
// La chiusura è ricordata in localStorage per `gameId` (stessa tolleranza degli altri accessi allo
// storage: se manca o lancia, il modale si ripropone a ogni caricamento ma l'app funziona).

const KEY_PREFIX = 'ciufciuf-rules-seen:';

/** `true` se su questo telefono le regole speciali di `gameId` sono già state chiuse con «Ho capito». */
export function hasSeenRules(gameId: string): boolean {
  if (gameId === '') return false;
  try {
    return globalThis.localStorage?.getItem(KEY_PREFIX + gameId) === '1';
  } catch (e) {
    console.warn('[storage] lettura fallita:', KEY_PREFIX + gameId, e);
    return false;
  }
}

/** Ricorda che le regole speciali di `gameId` sono state viste. */
export function markRulesSeen(gameId: string): void {
  if (gameId === '') return;
  try {
    globalThis.localStorage?.setItem(KEY_PREFIX + gameId, '1');
  } catch (e) {
    console.warn('[storage] scrittura fallita:', KEY_PREFIX + gameId, e);
  }
}

/**
 * Il modale delle regole si apre da solo a inizio partita (draft iniziale) se il ruleset ha regole
 * speciali e non sono già state viste; la riapertura manuale (📜) è gestita a parte.
 */
export function autoOpensRules(
  rulesCount: number,
  gameState: string,
  seen: boolean,
): boolean {
  return rulesCount > 0 && gameState === 'INITIAL_DRAFT' && !seen;
}
