// Client REST per la lobby CiufCiuf!.
//
// Copre: creazione/join/reassign/start partita, seed opzionale (solo dev), `hostToken` per start.
// Persistenza identità (playerId+token) per partita in localStorage con scadenza di 24 ore, per il
// reconnect dopo standby; l'`hostToken` vive solo in sessionStorage.

import type { PlayerColor } from './proto';
import { isAllowedHost, currentHostEnv, type HostEnv } from './server-host';
import { isRecord } from './guards';

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

/** Identità nuova per un nome di una partita salvata (rilasciata all'host). */
export interface ReassignedPlayer {
  playerId: string;
  token: string;
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
  /** Nomi del salvataggio che non sono ancora rientrati (solo lobby di resume). */
  missingPlayers?: string[];
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
  detail: string | null;
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

/** Partita salvata riprendibile (`GET /api/saves`). */
export interface SavedGameItem {
  gameId: string;
  name: string;
  rulesetId: string;
  /** Nomi dei giocatori del salvataggio. */
  players: string[];
  createdAt: string;
  updatedAt: string;
}

// ── Accesso a localStorage ───────────────────────────────────────────
// localStorage può mancare o lanciare (modalità privata, quota piena, cookie bloccati):
// in quel caso l'app funziona senza persistenza.

function readStorage(key: string): string | null {
  try {
    return globalThis.localStorage?.getItem(key) ?? null;
  } catch (e) {
    console.warn('[storage] lettura fallita:', key, e);
    return null;
  }
}

function writeStorage(key: string, value: string): void {
  try {
    globalThis.localStorage?.setItem(key, value);
  } catch (e) {
    console.warn('[storage] scrittura fallita:', key, e);
  }
}

function removeStorage(key: string): void {
  try {
    globalThis.localStorage?.removeItem(key);
  } catch (e) {
    console.warn('[storage] rimozione fallita:', key, e);
  }
}

function sessionGet(key: string): string | null {
  try {
    return globalThis.sessionStorage?.getItem(key) ?? null;
  } catch (e) {
    console.warn('[storage] lettura sessione fallita:', key, e);
    return null;
  }
}

function sessionSet(key: string, value: string): void {
  try {
    if (value) globalThis.sessionStorage?.setItem(key, value);
    else globalThis.sessionStorage?.removeItem(key);
  } catch (e) {
    console.warn('[storage] scrittura sessione fallita:', key, e);
  }
}

function readJson(key: string): unknown {
  const raw = readStorage(key);
  if (!raw) return null;
  try {
    return JSON.parse(raw);
  } catch (e) {
    console.warn('[storage] JSON non valido:', key, e);
    return null;
  }
}

// ── Identità del giocatore (una per partita) ─────────────────────────

const IDENTITY_PREFIX = 'ciufciuf-identity:';
const LAST_IDENTITY_KEY = 'ciufciuf-last-identity';

/** Un'identità salvata più di 24 ore fa viene ignorata. */
export const IDENTITY_MAX_AGE_MS = 24 * 60 * 60 * 1000;

export interface StoredIdentity {
  playerId: string;
  token: string;
  gameId: string;
  host: string;
  /** Nome del giocatore, se noto (mostrato nella schermata di riconnessione). */
  name?: string;
}

/**
 * Salva l'identità sotto la chiave della sua partita; con `remember` (default) la ricorda anche
 * come ultima usata, cioè la partita a cui ci si riconnette in automatico.
 */
export function saveIdentity(
  identity: StoredIdentity,
  now: number = Date.now(),
  remember: boolean = true,
): void {
  writeStorage(IDENTITY_PREFIX + identity.gameId, JSON.stringify({ ...identity, savedAt: now }));
  if (remember) writeStorage(LAST_IDENTITY_KEY, identity.gameId);
}

/**
 * Identità salvata per `gameId` (default: l'ultima partita usata).
 * Un'identità scaduta, senza `savedAt` o il cui host non coincide con l'ambiente corrente
 * viene ignorata.
 */
export function loadIdentity(
  gameId?: string,
  env: HostEnv = currentHostEnv(),
  now: number = Date.now(),
): StoredIdentity | null {
  const id = gameId ?? readStorage(LAST_IDENTITY_KEY);
  if (!id) return null;
  const v = readJson(IDENTITY_PREFIX + id);
  if (!isRecord(v)) return null;
  const { playerId, token, gameId: storedGame, host, savedAt, name } = v;
  if (
    typeof savedAt !== 'number' || !Number.isFinite(savedAt) ||
    now - savedAt > IDENTITY_MAX_AGE_MS || savedAt > now + IDENTITY_MAX_AGE_MS ||
    typeof playerId !== 'string' || !playerId ||
    typeof token !== 'string' || !token ||
    typeof storedGame !== 'string' || storedGame !== id ||
    typeof host !== 'string' || !isAllowedHost(host, env)
  ) {
    return null;
  }
  const identity: StoredIdentity = { playerId, token, gameId: storedGame, host };
  if (typeof name === 'string' && name) identity.name = name;
  return identity;
}

/** Rimuove l'identità di `gameId` (default: l'ultima partita usata). */
export function clearIdentity(gameId?: string): void {
  const id = gameId ?? readStorage(LAST_IDENTITY_KEY);
  if (!id) return;
  removeStorage(IDENTITY_PREFIX + id);
  if (readStorage(LAST_IDENTITY_KEY) === id) removeStorage(LAST_IDENTITY_KEY);
}

// ── Ultimo nome giocatore usato (prefill del JoinScreen) ────────────

const NAME_KEY = 'ciufciuf-name';

export function savePlayerName(name: string): void {
  writeStorage(NAME_KEY, name);
}

export function loadPlayerName(): string {
  return readStorage(NAME_KEY) ?? '';
}

// ── Ultima partita vista dalla table view (attach dopo reload) ───────

const LAST_GAME_KEY = 'ciufciuf-last-game';

export interface StoredLastGame {
  gameId: string;
  /** Vuoto se la TV si è agganciata a una partita non sua (niente Start). */
  hostToken: string;
}

const LAST_HOST_TOKEN_KEY = 'ciufciuf-last-host-token';

/** L'id partita resta in localStorage; l'`hostToken` solo in sessionStorage (muore con la scheda). */
export function saveLastGame(last: StoredLastGame): void {
  writeStorage(LAST_GAME_KEY, JSON.stringify({ gameId: last.gameId }));
  sessionSet(LAST_HOST_TOKEN_KEY, last.hostToken);
}

export function loadLastGame(): StoredLastGame | null {
  const v = readJson(LAST_GAME_KEY);
  if (!isRecord(v) || typeof v.gameId !== 'string' || !v.gameId) return null;
  return { gameId: v.gameId, hostToken: sessionGet(LAST_HOST_TOKEN_KEY) ?? '' };
}

export function clearLastGame(): void {
  removeStorage(LAST_GAME_KEY);
  sessionSet(LAST_HOST_TOKEN_KEY, '');
}

// ── Seed (solo dev) ──────────────────────────────────────────────────

/** Seed dal campo di testo: `undefined` se vuoto, `null` se non è un intero non negativo sicuro. */
export function parseSeed(raw: string): number | undefined | null {
  const text = raw.trim();
  if (text === '') return undefined;
  if (!/^\d+$/.test(text)) return null;
  const n = Number(text);
  return Number.isSafeInteger(n) ? n : null;
}

// ── Errori ───────────────────────────────────────────────────────────

/** Testi italiani per i codici di errore REST che il server può non accompagnare da un dettaglio. */
const ERROR_TEXT: Record<string, string> = {
  GAME_NOT_FOUND: 'La partita non esiste più',
  RULESET_NOT_FOUND: 'Mappa non trovata sul server',
  GAME_FULL: 'Partita al completo',
  NAME_TAKEN: 'Nome già in uso',
  COLOR_TAKEN: 'Questo colore è già stato scelto da un altro giocatore',
  NAME_NOT_IN_SAVE: 'Questo nome non fa parte della partita salvata',
  RESUME_TOKEN_MISMATCH:
    'Identità non riconosciuta: chiedi alla TV di riassegnarti il posto',
  UNAUTHORIZED: 'Non autorizzato',
  TOO_MANY_GAMES: 'Troppe partite attive sul server: riprova più tardi',
};

export class LobbyError extends Error {
  constructor(
    public status: number,
    message: string,
    /** Codice di errore del server (`error` nel corpo JSON), se presente. */
    public code: string | null = null,
  ) {
    super(message);
    this.name = 'LobbyError';
  }
}

// ── Client REST ──────────────────────────────────────────────────────

/** Tempo massimo per caricare la mappa. */
export const MAP_FETCH_TIMEOUT_MS = 10_000;
export const MAP_TIMEOUT_MESSAGE = 'Timeout caricamento mappa';

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
    signal?: AbortSignal,
  ): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const init: RequestInit = {
      method,
      headers: { 'Content-Type': 'application/json' },
    };
    if (body !== undefined) {
      init.body = JSON.stringify(body);
    }
    if (signal) init.signal = signal;
    const resp = await fetch(url, init);
    if (!resp.ok) {
      let code: string | null = null;
      let detail: string | null = null;
      try {
        const err = (await resp.json()) as Partial<ApiError>;
        code = typeof err.error === 'string' ? err.error : null;
        detail = typeof err.detail === 'string' ? err.detail : null;
      } catch (e) {
        console.warn('[lobby] corpo di errore non JSON:', e);
      }
      const message =
        detail ||
        (code !== null ? ERROR_TEXT[code] : undefined) ||
        code ||
        resp.statusText ||
        `Errore ${resp.status}`;
      throw new LobbyError(resp.status, message, code);
    }
    if (resp.status === 204) return undefined as T;
    if (!/^application\/(?:[\w.+-]+\+)?json\b/i.test(resp.headers.get('content-type') ?? '')) {
      throw new LobbyError(resp.status, 'Risposta inattesa dal server');
    }
    return resp.json() as Promise<T>;
  }

  private gamePath(gameId: string): string {
    return `/api/games/${encodeURIComponent(gameId)}`;
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

  // GET di una risorsa di mappa; chiude con un errore leggibile se il server non risponde entro
  // `MAP_FETCH_TIMEOUT_MS`.
  private async getWithMapTimeout(path: string): Promise<unknown> {
    const controller = new AbortController();
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      controller.abort();
    }, MAP_FETCH_TIMEOUT_MS);
    try {
      return await this.request<unknown>('GET', path, undefined, controller.signal);
    } catch (e) {
      if (timedOut) throw new Error(MAP_TIMEOUT_MESSAGE);
      throw e;
    } finally {
      clearTimeout(timer);
    }
  }

  // GET /api/rulesets/{id}/map — mappa e biglietti del ruleset (forma non validata: vedi map-loader).
  async getRulesetMap(rulesetId: string): Promise<unknown> {
    return this.getWithMapTimeout(`/api/rulesets/${encodeURIComponent(rulesetId)}/map`);
  }

  // GET /api/rulesets/{id}/layout — layout schematico della mappa del ruleset (forma non validata:
  // vedi map-loader).
  async getRulesetLayout(rulesetId: string): Promise<unknown> {
    return this.getWithMapTimeout(`/api/rulesets/${encodeURIComponent(rulesetId)}/layout`);
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
    return this.request<GameDetail>('GET', this.gamePath(gameId));
  }

  // POST /api/games/{id}/join — `token` è l'identità già posseduta (richiesto al resume)
  async joinGame(
    gameId: string,
    name: string,
    color?: PlayerColor,
    token?: string,
  ): Promise<JoinedPlayer> {
    const body: Record<string, string> = { name };
    if (color) body['color'] = color;
    if (token) body['token'] = token;
    return this.request<JoinedPlayer>('POST', `${this.gamePath(gameId)}/join`, body);
  }

  // POST /api/games/{id}/reassign — nuova identità per un nome del salvataggio (solo host)
  async reassignPlayer(
    gameId: string,
    hostToken: string,
    name: string,
  ): Promise<ReassignedPlayer> {
    return this.request<ReassignedPlayer>('POST', `${this.gamePath(gameId)}/reassign`, {
      hostToken,
      name,
    });
  }

  // POST /api/games/{id}/start
  async startGame(gameId: string, hostToken: string): Promise<void> {
    return this.request<void>('POST', `${this.gamePath(gameId)}/start`, { hostToken });
  }

  // GET /api/saves — partite salvate riprendibili (non già in lobby), dalla più recente
  async listSaves(): Promise<SavedGameItem[]> {
    const data = await this.request<{ saves: SavedGameItem[] }>('GET', '/api/saves');
    return data.saves;
  }

  // POST /api/saves/{id}/resume — riporta in lobby un salvataggio e rilascia l'`hostToken`
  async resumeSave(gameId: string): Promise<CreatedGame> {
    return this.request<CreatedGame>('POST', `/api/saves/${encodeURIComponent(gameId)}/resume`);
  }

  // DELETE /api/saves/{id} — elimina un salvataggio non attivo (il file resta come backup)
  async deleteSave(gameId: string): Promise<void> {
    return this.request<void>('DELETE', `/api/saves/${encodeURIComponent(gameId)}`);
  }
}
