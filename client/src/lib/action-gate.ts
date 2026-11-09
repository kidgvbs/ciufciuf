// Blocco anti doppio tap per le azioni del controller.
//
// Sulla LAN la risposta del server arriva in decine di ms, mentre un doppio
// tap dura 100-300 ms: un blocco solo "in volo" si riaprirebbe prima del
// secondo tap. Il gate resta quindi chiuso finché è arrivata la risposta E
// sono trascorsi `minMs`; a `maxMs` si riapre comunque.

export interface ActionGateOptions {
  minMs: number;
  maxMs: number;
  onChange?: (locked: boolean) => void;
}

export interface ActionGate {
  /** false se il gate è chiuso; altrimenti lo chiude e arma i timer. */
  tryAcquire(): boolean;
  /** Segnala l'arrivo della risposta. A gate aperto non fa nulla. */
  settle(): void;
  /** Cancella i timer e riapre il gate senza notificare `onChange`. */
  dispose(): void;
  readonly locked: boolean;
}

export function createActionGate({ minMs, maxMs, onChange }: ActionGateOptions): ActionGate {
  let locked = false;
  let responded = false;
  let minElapsed = false;
  let minTimer: ReturnType<typeof setTimeout> | null = null;
  let maxTimer: ReturnType<typeof setTimeout> | null = null;

  function clearTimers(): void {
    if (minTimer !== null) clearTimeout(minTimer);
    if (maxTimer !== null) clearTimeout(maxTimer);
    minTimer = null;
    maxTimer = null;
  }

  function unlock(): void {
    clearTimers();
    locked = false;
    onChange?.(false);
  }

  return {
    tryAcquire(): boolean {
      if (locked) return false;
      locked = true;
      responded = false;
      minElapsed = false;
      minTimer = setTimeout(() => {
        minElapsed = true;
        if (responded) unlock();
      }, minMs);
      maxTimer = setTimeout(unlock, maxMs);
      onChange?.(true);
      return true;
    },

    settle(): void {
      if (!locked) return;
      responded = true;
      if (minElapsed) unlock();
    },

    dispose(): void {
      clearTimers();
      locked = false;
    },

    get locked(): boolean {
      return locked;
    },
  };
}
