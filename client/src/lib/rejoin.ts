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
