// Flusso di join con timeout "lento": la richiesta non viene mai abortita.
//
// Allo scadere del timeout si avvisa l'utente ma la richiesta resta in corso (il server potrebbe
// averla già eseguita). Se poi fallisce, si controlla se il nome risulta già registrato.
// `cancel()` abbandona il flusso: la richiesta prosegue ma il suo esito viene ignorato dalla UI;
// l'identità di un join riuscito dopo l'annullamento viene comunque salvata (vedi
// `joinWithSavedIdentity`), così un nuovo tentativo con lo stesso nome la riusa.

import { LobbyError, type StoredIdentity } from './lobby';

export const JOIN_TIMEOUT_MS = 10_000;

export const SLOW_JOIN_MESSAGE = 'Connessione lenta…';
export const NAME_ALREADY_JOINED_MESSAGE = 'Nome già in partita: chiedi all\'host di riassegnarlo';
export const GAME_STARTED_MESSAGE =
  'Partita già avviata: possono rientrare solo i giocatori già presenti. Chiedi all\'host di riassegnarti dalla TV';
export const SEAT_REASSIGNED_MESSAGE =
  'Il tuo posto è stato riassegnato: chiedi un nuovo QR alla TV.';

export interface JoinFlow {
  /** Esegue `attempt`; risolve `true` se il join è riuscito. */
  run(): Promise<boolean>;
  /** Ferma il timer del join (idempotente). */
  stopJoinTimer(): void;
  /** Abbandona il join in corso: nessun callback, esito della richiesta ignorato (idempotente). */
  cancel(): void;
}

export interface JoinFlowOptions {
  /** `isCancelled()` diventa vero dopo `cancel()`: l'esito non va più applicato. */
  attempt: (isCancelled: () => boolean) => Promise<void>;
  /** Nomi già registrati nella partita (`GET /api/games/{id}`). */
  registeredNames: () => Promise<string[]>;
  name: string;
  onSlow: () => void;
  onError: (message: string) => void;
  timeoutMs?: number;
}

export function createJoinFlow(opts: JoinFlowOptions): JoinFlow {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let cancelled = false;
  const isCancelled = () => cancelled;

  function stopJoinTimer(): void {
    if (timer) clearTimeout(timer);
    timer = null;
  }

  function cancel(): void {
    cancelled = true;
    stopJoinTimer();
  }

  async function run(): Promise<boolean> {
    let slow = false;
    stopJoinTimer();
    timer = setTimeout(() => {
      slow = true;
      if (!cancelled) opts.onSlow();
    }, opts.timeoutMs ?? JOIN_TIMEOUT_MS);
    try {
      await opts.attempt(isCancelled);
      stopJoinTimer();
      return !cancelled;
    } catch (e: unknown) {
      stopJoinTimer();
      if (cancelled) return false;
      let message = e instanceof Error ? e.message : 'Errore';
      // Posto riassegnato: il messaggio è già quello giusto, la verifica del nome lo falserebbe.
      const seatReassigned = e instanceof LobbyError && e.code === 'RESUME_TOKEN_MISMATCH';
      if (slow && !seatReassigned) {
        try {
          const names = await opts.registeredNames();
          if (names.some((n) => sameName(n, opts.name))) message = NAME_ALREADY_JOINED_MESSAGE;
        } catch (lookupError) {
          console.warn('[join] verifica del nome fallita:', lookupError);
        }
        if (cancelled) return false;
      }
      opts.onError(message);
      return false;
    }
  }

  return { run, stopJoinTimer, cancel };
}

// ── Identità salvata ─────────────────────────────────────────────────

export interface JoinIdentity {
  playerId: string;
  token: string;
}

export interface JoinIdentityOptions {
  gameId: string;
  host: string;
  name: string;
  /** `isCancelled()` è vero se l'utente ha annullato il tentativo. */
  isCancelled: () => boolean;
  /** POST di join; `token` è quello già salvato per la partita, se c'è. */
  post: (token: string | undefined) => Promise<JoinIdentity>;
  loadIdentity: (gameId: string) => StoredIdentity | null;
  /** Rimuove l'identità salvata di una partita (il posto è stato riassegnato). */
  clearIdentity: (gameId: string) => void;
  /** `remember` false: salva senza diventare l'ultima partita (riconnessione automatica). */
  saveIdentity: (identity: StoredIdentity, now: number, remember: boolean) => void;
  now?: () => number;
}

/** Stesso nome secondo il client: trim, NFKC, spazi ridotti a uno e minuscole (come il server). */
export function sameName(a: string, b: string): boolean {
  return nameKey(a) === nameKey(b);
}

function nameKey(name: string): string {
  return name.normalize('NFKC').split(/\s+/).filter(Boolean).join(' ').toLowerCase();
}

/**
 * Join con l'identità salvata per la partita. Ritorna l'identità da applicare, oppure `null` se il
 * tentativo è stato annullato: in tal caso un join riuscito tardi viene salvato (senza diventare
 * l'ultima partita e senza sovrascrivere un'identità già presente) e la UI non viene toccata.
 * Se il server risponde NAME_TAKEN e l'identità salvata ha lo stesso nome, la registrazione è
 * nostra (es. join annullato ma riuscito): si riusa l'identità salvata.
 * Se risponde RESUME_TOKEN_MISMATCH (posto riassegnato) l'identità salvata dello stesso nome viene rimossa.
 */
export async function joinWithSavedIdentity(
  opts: JoinIdentityOptions,
): Promise<JoinIdentity | null> {
  const saved = opts.loadIdentity(opts.gameId);
  let result: JoinIdentity;
  try {
    result = await opts.post(saved?.token);
  } catch (e: unknown) {
    const mine = saved?.name !== undefined && sameName(saved.name, opts.name);
    if (saved && mine && !opts.isCancelled() && e instanceof LobbyError && e.code === 'NAME_TAKEN') {
      return { playerId: saved.playerId, token: saved.token };
    }
    if (saved && mine && e instanceof LobbyError && e.code === 'RESUME_TOKEN_MISMATCH') {
      // L'identità salvata (dello stesso nome) non vale più: va tolta, altrimenti il rientro resta
      // bloccato per 24 ore. Con un nome diverso l'identità non c'entra e resta intatta.
      opts.clearIdentity(opts.gameId);
      throw new LobbyError(e.status, SEAT_REASSIGNED_MESSAGE, e.code);
    }
    if (e instanceof LobbyError && e.code === 'GAME_STARTED') {
      throw new LobbyError(e.status, GAME_STARTED_MESSAGE, e.code);
    }
    throw e;
  }
  if (opts.isCancelled()) {
    if (!opts.loadIdentity(opts.gameId)) {
      const now = (opts.now ?? Date.now)();
      opts.saveIdentity(
        {
          playerId: result.playerId,
          token: result.token,
          gameId: opts.gameId,
          host: opts.host,
          name: opts.name,
        },
        now,
        false,
      );
    }
    return null;
  }
  return { playerId: result.playerId, token: result.token };
}
