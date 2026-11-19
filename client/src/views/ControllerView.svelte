<script lang="ts">
  // Controller view (CV, telefono): join, reconnect, game UI.
  //
  // Flusso:
  //   1. join     → JoinScreen (inserisci nome + gameId)
  //   2. waiting  → Lobby (attesa start)
  //   3. game     → ControllerGameView (mano, biglietti, azioni)
  //   4. gameover → Punteggio finale
  //
  // Persistenza identità in localStorage per reconnect dopo standby.

  import { onMount, untrack } from 'svelte';
  import { publicStore, privateStore, connectionStore, applySnapshot, resetStores } from '../lib/stores.svelte.ts';
  import {
    LobbyClient,
    saveIdentity,
    loadIdentity,
    clearIdentity,
    forgetLastIdentity,
    savePlayerName,
  } from '../lib/lobby';
  import { gameMap } from '../lib/game-map.svelte.ts';
  import { joinWithSavedIdentity } from '../lib/join-flow';
  import { createGameConnection, type GameConnection, type FatalReason } from '../lib/ws';
  import { currentHostEnv } from '../lib/server-host';
  import { parseRejoinHash } from '../lib/rejoin';
  import { createActionGate } from '../lib/action-gate';
  import { notConnectedError, fatalMessage } from '../lib/errors';
  import { rulesetCatalog } from '../lib/rulesets.svelte.ts';
  import type { Snapshot, PublicState, PrivateState, ErrorPayload, InboundAction, PlayerColor } from '../lib/proto';
  import { PLAYER_COLOR_CSS } from '../lib/colors';
  import JoinScreen from '../lib/components/JoinScreen.svelte';
  import ControllerGameView from '../lib/components/ControllerGameView.svelte';
  import MapSvg from '../lib/components/MapSvg.svelte';
  import FinalScores from '../lib/components/FinalScores.svelte';
  import ConfirmDialog from '../lib/components/ConfirmDialog.svelte';

  // ── Stato connessione ───────────────────────────────────────────
  // Il server è sempre quello che serve la pagina (in dev lo raggiunge il proxy di Vite).
  let host = $state(currentHostEnv().origin);
  let gameId = $state('');
  let playerId = $state('');
  let token = $state('');
  let connection: GameConnection | null = $state(null);
  // Messaggio mostrato nella JoinScreen dopo una connessione abbandonata.
  let notice = $state('');
  let lobbyError = $state('');
  // Nome dell'identità salvata, mostrato nella schermata di riconnessione automatica.
  let playerName = $state('');

  // Anti doppio tap su tutte le azioni di gioco (ogni tap doppio consumerebbe
  // una pescata, o invierebbe due volte la stessa rotta/scelta).
  let drawBusy = $state(false);
  const drawGate = createActionGate({
    minMs: 350,
    maxMs: 3000,
    onChange: (locked) => { drawBusy = locked; },
  });

  // ── Derived ────────────────────────────────────────────────────
  const gameState = $derived(publicStore.state);
  const mySummary = $derived(
    publicStore.playersSummary.find((p) => p.player_id === playerId),
  );
  const lobby = $derived(new LobbyClient(host));

  let lobbyPlayers = $state<Array<{ playerId: string; name: string; color: PlayerColor }>>([]);

  // ── Avvio: controlla localStorage (una sola volta) ─────────────
  // Questo restore NON deve essere un $effect tracciato: connectPlayer
  // legge `connection` e la riscrive nella stessa run → con $effect il loop
  // reattivo chiude/riapre il WS all'infinito. onMount gira una volta sola,
  // fuori da qualunque contesto reattivo.
  onMount(() => {
    rulesetCatalog.load(host);
    // Identità consegnata dalla TV via QR: la salviamo e togliamo il token dall'URL.
    const rejoin = parseRejoinHash(window.location.hash);
    if (rejoin) {
      saveIdentity({ ...rejoin, host });
      window.history.replaceState(null, '', window.location.pathname + window.location.search);
    }
    const saved = loadIdentity(rejoin?.gameId);
    if (saved) {
      host = saved.host;
      gameId = saved.gameId;
      playerId = saved.playerId;
      token = saved.token;
      playerName = saved.name ?? '';
      privateStore.playerId = playerId;
      // Prova reconnect
      connectPlayer(true);
    }
  });

  // ── Vista corrente ─────────────────────────────────────────────
  const view = $derived.by((): 'join' | 'waiting' | 'game' | 'gameover' => {
    if (publicStore.gameId === '') return 'join';
    if (gameState === 'WAITING_FOR_PLAYERS') return 'waiting';
    if (gameState === 'GAME_OVER') return 'gameover';
    return 'game';
  });

  // Nome del bonus a fine partita: se il catalogo non era arrivato, riprova.
  $effect(() => {
    if (view === 'gameover') untrack(() => rulesetCatalog.load(host));
  });

  // Polling giocatori in lobby
  $effect(() => {
    if (view === 'waiting' && gameId) {
      const interval = setInterval(refreshLobby, 3000);
      return () => clearInterval(interval);
    }
  });

  // ── Connessione / riconnessione WS ─────────────────────────────
  // Identità della connessione corrente, per la guardia di idempotenza.
  let connectionKey: string | null = null;

  function connectPlayer(reconnecting: boolean) {
    if (!gameId || !playerId || !token) return;
    // Guardia di idempotenza: se esiste già una connessione per la stessa
    // (gameId, playerId, token) non ricrearla — il reconnect interno di
    // ws.ts (backoff + visibilitychange) gestisce da solo le cadute.
    const key = `${gameId}${playerId}${token}`;
    if (connection && connectionKey === key) return;
    if (connection) connection.close();
    resetStores();
    privateStore.playerId = playerId;
    connectionStore.reconnecting = reconnecting;

    connection = createGameConnection({
      host,
      gameId,
      playerId,
      token,
      callbacks: {
        onSnapshot(snap: Snapshot) {
          applySnapshot(snap);
          if (snap.private) {
            privateStore.apply(snap.private);
          }
          connectionStore.reconnecting = false;
          drawGate.settle();
        },
        onPublicState(ps: PublicState) {
          publicStore.apply(ps);
        },
        onPrivateState(ps: PrivateState) {
          privateStore.apply(ps);
          drawGate.settle();
        },
        onError(err: ErrorPayload) {
          connectionStore.lastError = err;
          drawGate.settle();
        },
        onConnectionChange(connected: boolean) {
          connectionStore.connected = connected;
          if (!connected) {
            connectionStore.reconnecting = true;
            drawGate.settle();
          }
        },
        onFatal(reason: FatalReason) {
          // Una versione incompatibile non invalida l'identità: basta ricaricare la pagina.
          leaveGame(reason !== 'PROTOCOL_VERSION');
          notice = fatalMessage(reason);
        },
      },
    });
    connectionKey = key;
  }

  // Mappa e biglietti della partita corrente, dal ruleset indicato dal server.
  $effect(() => {
    const gid = gameId;
    if (!gid) {
      gameMap.reset();
      return;
    }
    untrack(() => {
      const client = lobby;
      gameMap.track(host, async () => (await client.getGame(gid)).rulesetId);
    });
  });

  // Chiudi la connessione WS allo smontaggio (popstate in App.svelte rimonta le viste).
  // dispose dopo close: close() notifica onConnectionChange(false) → settle().
  $effect(() => () => { connection?.close(); connection = null; connectionKey = null; drawGate.dispose(); });

  // ── Join ───────────────────────────────────────────────────────
  // Al resume il server richiede il token originale: lo inviamo se abbiamo un'identità
  // salvata per la stessa partita. Un join riuscito dopo "Annulla" viene salvato senza
  // cambiare vista (vedi `joinWithSavedIdentity`).
  async function onJoin(
    name: string,
    gid: string,
    color: PlayerColor | undefined,
    isCancelled: () => boolean,
  ) {
    notice = '';
    const result = await joinWithSavedIdentity({
      gameId: gid,
      host,
      name,
      isCancelled,
      post: (savedToken) => lobby.joinGame(gid, name, color, savedToken),
      loadIdentity,
      clearIdentity,
      saveIdentity,
    });
    if (!result) return;
    gameId = gid;
    playerId = result.playerId;
    token = result.token;
    saveIdentity({ playerId, token, gameId: gid, host, name });
    playerName = name;
    savePlayerName(name);
    connectPlayer(false);
  }

  // ── Azioni ─────────────────────────────────────────────────────
  /** Invia un'azione; `false` se non è partita (doppio tap o socket chiuso). */
  function sendAction(action: InboundAction): boolean {
    if (!connection?.connected) {
      connectionStore.lastError = notConnectedError();
      return false;
    }
    if (action.actionType !== 'REQUEST_SNAPSHOT' && !drawGate.tryAcquire()) return false;
    if (!connection.send(action)) {
      drawGate.settle();
      connectionStore.lastError = notConnectedError();
      return false;
    }
    return true;
  }

  let confirmingLeave = $state(false);

  function confirmLeave() {
    if (view === 'game') {
      confirmingLeave = true;
      return;
    }
    onLeave();
  }

  function onLeave() {
    confirmingLeave = false;
    leaveGame(true);
    notice = '';
  }

  /**
   * "Esci" dalla partita in corso: l'identità resta, così si può rientrare scegliendo la partita
   * dalla JoinScreen, ma al prossimo caricamento non ci si riconnette in automatico.
   */
  function onLeaveKeepingSeat() {
    confirmingLeave = false;
    if (gameId) forgetLastIdentity(gameId);
    leaveGame(false);
    notice = '';
  }

  /** Chiude la connessione e torna alla JoinScreen; `forget` cancella l'identità salvata. */
  function leaveGame(forget: boolean) {
    if (connection) {
      connection.close();
      connection = null;
      connectionKey = null;
    }
    if (forget && gameId) clearIdentity(gameId);
    resetStores();
    gameId = '';
    playerId = '';
    token = '';
    lobbyPlayers = [];
    lobbyError = '';
  }

  // ── Lobby ──────────────────────────────────────────────────────
  async function refreshLobby() {
    if (!gameId) return;
    try {
      const detail = await lobby.getGame(gameId);
      lobbyPlayers = detail.players.map((p) => ({
        playerId: p.playerId,
        name: p.name,
        color: p.color as PlayerColor,
      }));
      lobbyError = '';
    } catch (e: unknown) {
      console.warn('[controller] aggiornamento lobby fallito:', e);
      lobbyError = 'Impossibile aggiornare i giocatori in lobby';
    }
  }
</script>

<div class="cv-root">
  {#if view === 'join' && connection !== null}
    <div class="waiting-screen">
      <h1>🚂 CiufCiuf!</h1>
      <div class="waiting-card" role="status" aria-live="polite">
        <p class="waiting-msg">
          Riconnessione come <strong>{playerName || 'giocatore salvato'}</strong>…
        </p>
        <button class="leave-btn btn" onclick={onLeave}>Non sono io</button>
      </div>
    </div>

  {:else if view === 'join'}
    <JoinScreen {host} {onJoin} notice={notice} />

  {:else if view === 'waiting'}
    <div class="waiting-screen">
      <h1>🚂 CiufCiuf!</h1>
      <div class="waiting-card">
        <p class="waiting-msg">
          Connesso come
          <strong>{mySummary?.name ?? playerId}</strong>
        </p>
        <div
          class="my-color"
          style="background:{mySummary
            ? PLAYER_COLOR_CSS[mySummary.color]
            : '#666'}"
        ></div>

        <h2>Giocatori in lobby</h2>
        <div class="player-list">
          {#each lobbyPlayers as p}
            <div
              class="player-chip"
              style="border-color:{PLAYER_COLOR_CSS[p.color]}"
            >
              <span
                class="chip-dot"
                style="background:{PLAYER_COLOR_CSS[p.color]}"
              ></span>
              <span class="chip-name">{p.name}</span>
            </div>
          {/each}
          {#if lobbyPlayers.length === 0}
            <p class="muted">In attesa...</p>
          {/if}
        </div>
        {#if lobbyError}
          <p class="muted" role="status">{lobbyError}</p>
        {/if}

        <p class="waiting-hint">
          Aspetta che l'host avvii la partita...
        </p>

        <button class="leave-btn btn" onclick={onLeave}> Esci </button>
      </div>

      <div role="status" aria-live="polite">
        {#if !connectionStore.connected}
          <div class="disconnected">⚠️ Disconnesso &mdash; riconnessione in corso...</div>
        {/if}
      </div>
    </div>

  {:else if view === 'gameover'}
    <div class="gameover-screen">
      <h1>🏁 Fine partita!</h1>
      {#if publicStore.finalScores}
        <FinalScores
          scores={publicStore.finalScores}
          players={publicStore.playersSummary}
          {playerId}
          longestPathPlayerIds={publicStore.longestPathPlayerIds}
          longestPathName={rulesetCatalog.hasLongestPath(gameMap.rulesetId) ? rulesetCatalog.longestPathLabel(gameMap.rulesetId) : null}
          globetrotterName={rulesetCatalog.hasGlobetrotter(gameMap.rulesetId) ? rulesetCatalog.globetrotterLabel(gameMap.rulesetId) : null}
          mandalaName={rulesetCatalog.hasMandala(gameMap.rulesetId) ? rulesetCatalog.mandalaLabel(gameMap.rulesetId) : null}
          mapData={gameMap.state.data}
        >
          <!-- Mappa a fine partita: il percorso più lungo è evidenziato (glow dorato). -->
          <div class="gameover-map"><MapSvg compact /></div>
        </FinalScores>
      {/if}
      <button class="leave-btn btn" onclick={onLeave}> Nuova partita </button>
    </div>

  {:else}
    <ControllerGameView {playerId} {sendAction} onLeave={confirmLeave} {drawBusy} />
  {/if}

  {#if confirmingLeave}
    <ConfirmDialog
      title="Uscire dalla partita?"
      message="Potrai rientrare finché la partita è in corso: scegli di nuovo la partita dall'elenco."
      confirmLabel="Esci"
      onConfirm={onLeaveKeepingSeat}
      onCancel={() => (confirmingLeave = false)}
    />
  {/if}
</div>

<style>
  .cv-root {
    min-height: 100vh;
    min-height: 100dvh;
    background: var(--chrome-bg-0);
    background-image: var(--table-felt);
    color: var(--chrome-text);
    font-family: system-ui, sans-serif;
  }

  /* Waiting screen */
  .waiting-screen {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 2rem 1rem;
  }
  .waiting-screen h1 {
    font-size: 2rem;
    color: var(--gold);
    margin-bottom: 1rem;
    text-shadow: 0 2px 6px rgba(0, 0, 0, 0.5);
  }
  .waiting-card {
    background: var(--chrome-bg-1);
    border: 1px solid var(--chrome-line);
    border-radius: 12px;
    box-shadow: var(--shadow-panel);
    padding: 1.5rem;
    width: 100%;
    max-width: 360px;
    text-align: center;
  }
  .waiting-msg {
    font-size: 1.1rem;
    margin: 0 0 0.5rem;
  }
  .my-color {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    margin: 0.5rem auto;
    border: 2px solid var(--chrome-line);
    box-shadow: 0 2px 5px rgba(0, 0, 0, 0.4);
  }
  .waiting-card h2 {
    font-size: 1rem;
    color: var(--gold);
    margin: 1rem 0 0.5rem;
  }
  .player-list {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    justify-content: center;
    margin-bottom: 1rem;
  }
  .player-chip {
    display: flex;
    align-items: center;
    min-width: 0;
    max-width: 100%;
    gap: 0.3rem;
    padding: 0.3rem 0.7rem;
    background: var(--chrome-bg-2);
    border: 2px solid;
    border-radius: 20px;
    font-size: 0.9rem;
  }
  .chip-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip-dot {
    display: inline-block;
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  .waiting-hint {
    color: var(--chrome-faint);
    font-size: 0.85rem;
  }
  .muted {
    color: var(--chrome-faint);
    font-size: 0.85rem;
  }
  .disconnected {
    margin-top: 1rem;
    background: #a93226;
    color: #fff;
    padding: 0.5rem 1rem;
    border-radius: 8px;
    font-weight: bold;
  }

  /* Game over screen */
  .gameover-screen {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 2rem 1rem;
    text-align: center;
  }
  .gameover-screen h1 {
    color: var(--gold);
    margin-bottom: 1.5rem;
    text-shadow: 0 2px 6px rgba(0, 0, 0, 0.5);
  }
  .gameover-map {
    width: 100%;
    max-width: 560px;
    height: 260px;
    margin-bottom: 1rem;
    text-align: left;
  }

  /* Shared */
  .leave-btn {
    background: #a93226;
    color: #fff;
    padding: 0.5rem 1.5rem;
    font-size: 0.9rem;
    margin-top: 1rem;
  }
</style>