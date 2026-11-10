// Client REST per la lobby CiufCiuf! (7 endpoint).
//
// Copre: creazione/join partita, seed opzionale per test, `hostToken` per start (E9).
// Persistenza identità (playerId+token) in localStorage per reconnect dopo standby.

import type { PlayerColor } from './proto';

// ── Tipi di risposta ─────────────────────────────────────────────────

export interface CreatedGame {
  gameId: string;
  hostToken: string;
  rulesetId: string;
  maxPlayers: number;
}

export interface JoinedPlayer {
  playerId: string;
  token: string;
  wsUrl: string;
}

export interface GameListItem {
  gameId: string;
  name: string;
  rulesetId: string;
  playersJoined: number;
  maxPlayers: number;
  started: boolean;
}

export interface GameDetailPlayer {
  playerId: string;
  name: string;
  color: PlayerColor;
}

export interface GameDetail {
  gameId: string;
  name: string;
  rulesetId: string;
  players: GameDetailPlayer[];
  started: boolean;
  maxPlayers: number;
}

export interface LanCandidate {
  ip: string;
  url: string;
  preferred: boolean;
}

export interface LanInfo {
  port: number;
  candidates: LanCandidate[];
  fallback: string;
}

export interface ApiError {
  error: string;
  detail: string;
}

/** Riga della classifica generale (partite concluse archiviate). */
export interface StandingRow {
  name: string;
  games: number;
  wins: number;
  totalPoints: number;
  best: number;
}

export interface Standings {
  games: number;
  standings: StandingRow[];
}

// ── LocalStorage identity ────────────────────────────────────────────

const STORAGE_KEY = 'ciufciuf-identity';

export interface StoredIdentity {
  playerId: string;
  token: string;
  gameId: string;
  host: string;
}

export function saveIdentity(identity: StoredIdentity): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(identity));
}

export function loadIdentity(): StoredIdentity | null {
  const raw = localStorage.getItem(STORAGE_KEY);
  if (!raw) return null;
  try {
    return JSON.parse(raw) as StoredIdentity;
  } catch {
    return null;
  }
}

export function clearIdentity(): void {
  localStorage.removeItem(STORAGE_KEY);
}

// ── Ultimo nome giocatore usato (prefill del JoinScreen) ────────────

const NAME_KEY = 'ciufciuf-name';

export function savePlayerName(name: string): void {
  localStorage.setItem(NAME_KEY, name);
}

export function loadPlayerName(): string {
  return localStorage.getItem(NAME_KEY) ?? '';
}

// ── Ultima partita vista dalla table view (attach dopo reload) ───────

const LAST_GAME_KEY = 'ciufciuf-last-game';

export interface StoredLastGame {
  gameId: string;
  /** Vuoto se la TV si è agganciata a una partita non sua (niente Start). */
  hostToken: string;
}

export function saveLastGame(last: StoredLastGame): void {
  localStorage.setItem(LAST_GAME_KEY, JSON.stringify(last));
}

export function loadLastGame(): StoredLastGame | null {
  const raw = localStorage.getItem(LAST_GAME_KEY);
  if (!raw) return null;
  try {
    return JSON.parse(raw) as StoredLastGame;
  } catch {
    return null;
  }
}

export function clearLastGame(): void {
  localStorage.removeItem(LAST_GAME_KEY);
}

// ── Client REST ──────────────────────────────────────────────────────

export class LobbyClient {
  private baseUrl: string;

  constructor(host: string) {
    // host deve includere il protocollo (http:// o https://)
    this.baseUrl = host.replace(/\/$/, '');
  }

  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
  ): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const init: RequestInit = {
      method,
      headers: { 'Content-Type': 'application/json' },
    };
    if (body !== undefined) {
      init.body = JSON.stringify(body);
    }
    const resp = await fetch(url, init);
    if (!resp.ok) {
      let detail = resp.statusText;
      try {
        const err = (await resp.json()) as ApiError;
        detail = err.detail || err.error || detail;
      } catch { /* usa statusText */ }
      throw new LobbyError(resp.status, detail);
    }
    if (resp.status === 204) return undefined as T;
    return resp.json() as Promise<T>;
  }

  // GET /api/lan
  async getLan(): Promise<LanInfo> {
    return this.request<LanInfo>('GET', '/api/lan');
  }

  // GET /api/standings
  async getStandings(): Promise<Standings> {
    return this.request<Standings>('GET', '/api/standings');
  }

  // GET /api/profiles — nomi salvati (creati al primo join)
  async getProfiles(): Promise<string[]> {
    const data = await this.request<{ profiles: string[] }>('GET', '/api/profiles');
    return data.profiles;
  }

  // GET /qr?data=<url> → restituisce l'URL dell'immagine SVG
  qrUrl(data: string): string {
    return `${this.baseUrl}/qr?data=${encodeURIComponent(data)}`;
  }

  // POST /api/games
  async createGame(
    rulesetId: string,
    name: string,
    seed?: number,
  ): Promise<CreatedGame> {
    const body: Record<string, unknown> = { rulesetId, name };
    if (seed !== undefined) body['seed'] = seed;
    return this.request<CreatedGame>('POST', '/api/games', body);
  }

  // GET /api/games
  async listGames(): Promise<GameListItem[]> {
    const data = await this.request<{ games: GameListItem[] }>(
      'GET',
      '/api/games',
    );
    return data.games;
  }

  // GET /api/games/{id}
  async getGame(gameId: string): Promise<GameDetail> {
    return this.request<GameDetail>('GET', `/api/games/${gameId}`);
  }

  // POST /api/games/{id}/join
  async joinGame(
    gameId: string,
    name: string,
    color?: PlayerColor,
  ): Promise<JoinedPlayer> {
    const body: Record<string, string> = { name };
    if (color) body['color'] = color;
    return this.request<JoinedPlayer>(
      'POST',
      `/api/games/${gameId}/join`,
      body,
    );
  }

  // POST /api/games/{id}/start
  async startGame(gameId: string, hostToken: string): Promise<void> {
    return this.request<void>(
      'POST',
      `/api/games/${gameId}/start`,
      { hostToken },
    );
  }

  /** URL WebSocket per una partita (senza parametri di query). */
  wsBaseUrl(gameId: string): string {
    const protocol = this.baseUrl.startsWith('https') ? 'wss' : 'ws';
    const hostPart = this.baseUrl.replace(/^https?:\/\//, '');
    return `${protocol}://${hostPart}/ws/${gameId}`;
  }
}

// ── Errori ───────────────────────────────────────────────────────────

export class LobbyError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(`Lobby error ${status}: ${message}`);
    this.name = 'LobbyError';
  }
}