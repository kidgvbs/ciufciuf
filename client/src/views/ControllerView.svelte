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

  import { onMount } from 'svelte';
  import { publicStore, privateStore, connectionStore, applySnapshot, resetStores } from '../lib/stores.svelte.ts';
  import {
    LobbyClient,
    saveIdentity,
    loadIdentity,
    clearIdentity,
    savePlayerName,
    type StoredIdentity,
  } from '../lib/lobby';
  import { createGameConnection, type GameConnection } from '../lib/ws';
  import { createActionGate } from '../lib/action-gate';
  import type { Snapshot, PublicState, PrivateState, ErrorPayload, InboundAction, PlayerColor } from '../lib/proto';
  import { PLAYER_COLOR_CSS } from '../lib/colors';
  import JoinScreen from '../lib/components/JoinScreen.svelte';
  import ControllerGameView from '../lib/components/ControllerGameView.svelte';

  // ── Stato connessione ───────────────────────────────────────────
  // Default: stesso server che serve la pagina. Override: `/controller?server=http://<ip>:8080`.
  let host = $state(
    new URLSearchParams(window.location.search).get('server') ?? window.location.origin,
  );
  let gameId = $state('');
  let playerId = $state('');
  let token = $state('');
  let connection: GameConnection | null = $state(null);
  let view: 'join' | 'waiting' | 'game' | 'gameover' = $state('join');
  let joinError = $state('');

  // Anti doppio tap sulle pescate (ogni tap consumerebbe una pescata del turno).
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
  // P0: questo restore NON deve essere un $effect tracciato: connectPlayer
  // legge `connection` e la riscrive nella stessa run → con $effect il loop
  // reattivo chiude/riapre il WS all'infinito. onMount gira una volta sola,
  // fuori da qualunque contesto reattivo.
  onMount(() => {
    const saved = loadIdentity();
    if (saved && saved.playerId && saved.token && saved.gameId) {
      host = saved.host || host;
      gameId = saved.gameId;
      playerId = saved.playerId;
      token = saved.token;
      privateStore.playerId = playerId;
      // Prova reconnect
      connectPlayer(true);
    }
  });

  // ── Transizioni di stato ───────────────────────────────────────
  $effect(() => {
    const gs = publicStore.state;
    if (!gs || publicStore.gameId === '') return;

    if (gs === 'WAITING_FOR_PLAYERS') {
      view = 'waiting';
    } else if (gs === 'GAME_OVER') {
      view = 'gameover';
    } else {
      view = 'game';
    }
  });

  // Polling giocatori in lobby
  $effect(() => {
    if (view === 'waiting' && gameId) {
      const interval = setInterval(refreshLobby, 3000);
      return () => clearInterval(interval);
    }
  });

  // ── Connessione / riconnessione WS ─────────────────────────────
  // Identità della connessione corrente, per la guardia di idempotenza (P0).
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
        onSnapshot(snap: Snapshot, isReconnect: boolean) {
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
      },
    });
    connectionKey = key;
  }

  // F1: chiudi la connessione WS allo smontaggio (popstate in App.svelte rimonta le viste).
  // dispose dopo close: close() notifica onConnectionChange(false) → settle().
  $effect(() => () => { connection?.close(); connection = null; connectionKey = null; drawGate.dispose(); });

  // ── Join ───────────────────────────────────────────────────────
  async function onJoin(name: string, gid: string, color?: PlayerColor) {
    joinError = '';
    try {
      const result = await lobby.joinGame(gid, name, color);
      gameId = gid;
      playerId = result.playerId;
      token = result.token;
      saveIdentity({ playerId, token, gameId: gid, host });
      savePlayerName(name);
      connectPlayer(false);
    } catch (e: unknown) {
      joinError = e instanceof Error ? e.message : 'Errore join';
      throw e;
    }
  }

  // ── Azioni ─────────────────────────────────────────────────────
  function sendAction(action: InboundAction) {
    if (action.actionType === 'DRAW_TRAIN_CARD') {
      if (!connection?.connected) return;
      if (!drawGate.tryAcquire()) return;
    }
    connection?.send(action);
  }

  function onLeave() {
    if (connection) {
      connection.close();
      connection = null;
      connectionKey = null;
    }
    clearIdentity();
    resetStores();
    gameId = '';
    playerId = '';
    token = '';
    view = 'join';
    joinError = '';
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
    } catch {
      // ignorato
    }
  }
</script>

<div class="cv-root">
  {#if view === 'join'}
    <JoinScreen {host} {onJoin} />

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
              {p.name}
            </div>
          {/each}
          {#if lobbyPlayers.length === 0}
            <p class="muted">In attesa...</p>
          {/if}
        </div>

        <p class="waiting-hint">
          Aspetta che l'host avvii la partita...
        </p>

        <button class="leave-btn btn" onclick={onLeave}> Esci </button>
      </div>

      {#if !connectionStore.connected}
        <div class="disconnected">⚠️ Disconnesso</div>
      {/if}
    </div>

  {:else if view === 'gameover'}
    <div class="gameover-screen">
      <h1>🏁 Fine partita!</h1>
      {#if publicStore.finalScores}
        <div class="scores-list">
          {#each publicStore.finalScores as fs}
            {@const p = publicStore.playersSummary.find(
              (ps) => ps.player_id === fs.playerId,
            )}
            <div
              class="score-card"
              class:mine={fs.playerId === playerId}
              class:winner={publicStore.longestPathPlayerIds?.includes(
                fs.playerId,
              )}
            >
              <div class="score-header">
                <span
                  class="chip-dot"
                  style="background:{p
                    ? PLAYER_COLOR_CSS[p.color]
                    : '#666'}"
                ></span>
                <span class="score-name">{p?.name ?? fs.playerId}</span>
                <span class="score-total">{fs.total}</span>
              </div>
              <div class="score-detail">
                Rotte {fs.routePoints} · Biglietti +{fs.ticketCompleted}
                {#if fs.ticketFailed < 0} · {fs.ticketFailed}{/if}
                {#if fs.longestPathBonus > 0} · 🏆 +{fs.longestPathBonus}{/if}
              </div>
            </div>
          {/each}
        </div>
        {#if publicStore.longestPathPlayerIds}
          <div class="longest-banner">
            🏆 Longest Path:
            {publicStore.longestPathPlayerIds
              .map(
                (pid) =>
                  publicStore.playersSummary.find((ps) => ps.player_id === pid)
                    ?.name ?? pid,
              )
              .join(', ')}
          </div>
        {/if}
      {/if}
      <button class="leave-btn btn" onclick={onLeave}> Nuova partita </button>
    </div>

  {:else}
    <ControllerGameView {playerId} {sendAction} {onLeave} {drawBusy} />
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
    gap: 0.3rem;
    padding: 0.3rem 0.7rem;
    background: var(--chrome-bg-2);
    border: 2px solid;
    border-radius: 20px;
    font-size: 0.9rem;
  }
  .chip-dot {
    display: inline-block;
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
  .scores-list {
    width: 100%;
    max-width: 380px;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .score-card {
    background: var(--chrome-bg-1);
    border: 1px solid var(--chrome-line);
    border-radius: 8px;
    box-shadow: var(--shadow-panel);
    padding: 0.75rem;
    border-left: 4px solid var(--chrome-line);
    text-align: left;
  }
  .score-card.mine {
    border-left-color: var(--gold);
  }
  .score-card.winner {
    background: #2c3a1c;
  }
  .score-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .score-name {
    flex: 1;
    font-weight: 600;
  }
  .score-total {
    font-weight: bold;
    font-size: 1.3rem;
    color: var(--gold);
  }
  .score-detail {
    font-size: 0.8rem;
    color: var(--chrome-muted);
    margin-top: 0.25rem;
    margin-left: 22px;
  }
  .longest-banner {
    margin-top: 1rem;
    color: var(--gold);
    font-weight: bold;
    font-size: 1.1rem;
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