// Sequenza con cui la TV si aggancia a una partita creata o ripresa. Dopo ogni attesa si ricontrolla
// che la vista sia ancora montata, così una risposta tardiva non apre nessuna WebSocket.

export interface AdoptSteps {
  isAlive: () => boolean;
  /** Registra la partita (id, token, ultima partita ricordata, URL). */
  adopt: () => void;
  refreshLobby: () => Promise<void>;
  /** Apre la connessione spettatore. */
  connect: () => void;
}

export async function adoptGame({ isAlive, adopt, refreshLobby, connect }: AdoptSteps): Promise<void> {
  if (!isAlive()) return;
  adopt();
  await refreshLobby();
  if (!isAlive()) return;
  connect();
}
