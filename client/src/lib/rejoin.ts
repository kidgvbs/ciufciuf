// Identità consegnata al telefono tramite QR dalla TV (riassegnazione di un posto salvato).
//
// L'identità viaggia nel fragment dell'URL (`/controller#gameId=…&playerId=…&token=…`),
// che il browser non invia al server.

export interface RejoinIdentity {
  gameId: string;
  playerId: string;
  token: string;
}

/** URL del controller, a partire da `baseUrl` (es. `http://192.168.1.5:8080`), che porta l'identità. */
export function buildRejoinUrl(baseUrl: string, identity: RejoinIdentity): string {
  const params = new URLSearchParams({
    gameId: identity.gameId,
    playerId: identity.playerId,
    token: identity.token,
  });
  return `${baseUrl.replace(/\/$/, '')}/controller#${params.toString()}`;
}

/** Identità contenuta nel fragment (`location.hash`), o `null` se manca qualche campo. */
export function parseRejoinHash(hash: string): RejoinIdentity | null {
  const params = new URLSearchParams(hash.replace(/^#/, ''));
  const gameId = params.get('gameId');
  const playerId = params.get('playerId');
  const token = params.get('token');
  if (!gameId || !playerId || !token) return null;
  return { gameId, playerId, token };
}

/**
 * QR di rientro ancora utili: tiene la voce di un nome solo se è ancora in attesa di rientrare
 * (nome del salvataggio non rientrato, o giocatore del roster ancora disconnesso). Chi è tornato
 * connesso o è uscito dal roster perde la voce. Restituisce `links` stesso se non cambia nulla.
 */
export function pruneRejoinLinks(
  links: Record<string, string>,
  players: ReadonlyArray<{ name: string; connected: boolean }>,
  missingNames: readonly string[],
): Record<string, string> {
  const waiting = new Set(missingNames);
  for (const p of players) {
    if (!p.connected) waiting.add(p.name);
    else waiting.delete(p.name);
  }
  const names = Object.keys(links);
  if (names.every((n) => waiting.has(n))) return links;
  return Object.fromEntries(names.filter((n) => waiting.has(n)).map((n) => [n, links[n]!]));
}

export interface ReassignSeatOptions {
  /** Partita (e token host) catturati al momento del clic. */
  gameId: string;
  token: string;
  name: string;
  controllerBase: string;
  request: (gameId: string, token: string, name: string) => Promise<{ playerId: string; token: string }>;
  /** Partita attualmente agganciata dalla TV (vuota se l'ha lasciata). */
  currentGameId: () => string;
}

/**
 * Riassegna un posto e costruisce l'URL del QR di rientro. Ritorna `null` se, durante l'attesa,
 * la TV ha lasciato o cambiato partita: l'esito non va applicato a quella nuova.
 */
export async function reassignSeat(opts: ReassignSeatOptions): Promise<string | null> {
  const { gameId } = opts;
  const who = await opts.request(gameId, opts.token, opts.name);
  if (opts.currentGameId() !== gameId) return null;
  return buildRejoinUrl(opts.controllerBase, { gameId, playerId: who.playerId, token: who.token });
}
