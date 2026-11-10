// Flusso di join con timeout "lento": la richiesta non viene mai annullata.
//
// Allo scadere del timeout si avvisa l'utente ma la richiesta resta in corso (il server potrebbe
// averla già eseguita). Se poi fallisce, si controlla se il nome risulta già registrato.

export const JOIN_TIMEOUT_MS = 10_000;

export const SLOW_JOIN_MESSAGE = 'Connessione lenta…';
export const NAME_ALREADY_JOINED_MESSAGE = 'Nome già in partita: chiedi all\'host di riassegnarlo';

export interface JoinFlow {
  /** Esegue `attempt`; risolve `true` se il join è riuscito. */
  run(): Promise<boolean>;
  /** Ferma il timer del join (idempotente). */
  stopJoinTimer(): void;
}

export interface JoinFlowOptions {
  attempt: () => Promise<void>;
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

  function stopJoinTimer(): void {
    if (timer) clearTimeout(timer);
    timer = null;
  }

  async function run(): Promise<boolean> {
    let slow = false;
    stopJoinTimer();
    timer = setTimeout(() => {
      slow = true;
      opts.onSlow();
    }, opts.timeoutMs ?? JOIN_TIMEOUT_MS);
    try {
      await opts.attempt();
      stopJoinTimer();
      return true;
    } catch (e: unknown) {
      stopJoinTimer();
      let message = e instanceof Error ? e.message : 'Errore';
      if (slow) {
        try {
          const names = await opts.registeredNames();
          if (names.some((n) => sameName(n, opts.name))) message = NAME_ALREADY_JOINED_MESSAGE;
        } catch (lookupError) {
          console.warn('[join] verifica del nome fallita:', lookupError);
        }
      }
      opts.onError(message);
      return false;
    }
  }

  return { run, stopJoinTimer };
}
