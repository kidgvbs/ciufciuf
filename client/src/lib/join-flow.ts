// Flusso di join con timeout "lento": la richiesta non viene mai abortita.
//
// Allo scadere del timeout si avvisa l'utente ma la richiesta resta in corso (il server potrebbe
// averla già eseguita). Se poi fallisce, si controlla se il nome risulta già registrato.
// `cancel()` abbandona il flusso: la richiesta prosegue ma il suo esito viene ignorato.

export const JOIN_TIMEOUT_MS = 10_000;

export const SLOW_JOIN_MESSAGE = 'Connessione lenta…';
export const NAME_ALREADY_JOINED_MESSAGE = 'Nome già in partita: chiedi all\'host di riassegnarlo';

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

function sameName(a: string, b: string): boolean {
  return a.trim().toLowerCase() === b.trim().toLowerCase();
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
      if (slow) {
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
