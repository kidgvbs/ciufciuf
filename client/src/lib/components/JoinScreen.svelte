<script lang="ts">
  // Schermata di join: nome giocatore + selezione partita (da lobby o manuale).

  import { LobbyClient, loadPlayerName, type GameListItem } from '../lobby';
  import { PLAYER_COLOR_CSS } from '../colors';
  import type { PlayerColor } from '../proto';

  const ALL_COLORS: PlayerColor[] = ['RED', 'BLUE', 'GREEN', 'YELLOW', 'BLACK'];

  let { host, onJoin, onCancel }: {
    host: string;
    onJoin: (name: string, gameId: string, color?: PlayerColor) => Promise<void>;
    onCancel?: () => void;
  } = $props();

  // Nome precompilato dall'ultimo join (localStorage)
  let name = $state(loadPlayerName());
  let gameId = $state('');
  let color = $state<PlayerColor | ''>('');
  let takenColors = $state<Set<string>>(new Set());
  let games = $state<GameListItem[]>([]);
  let loading = $state(false);
  let error = $state('');
  let joining = $state(false);

  const lobby = $derived(new LobbyClient(host));

  async function loadGames() {
    loading = true;
    error = '';
    try {
      games = await lobby.listGames();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Errore';
    }
    loading = false;
  }

  $effect(() => {
    loadGames();
  });

  // Quando cambia la partita selezionata, recupera i colori già presi.
  $effect(() => {
    const gid = gameId.trim();
    color = '';
    if (!gid) {
      takenColors = new Set();
      return;
    }
    let cancelled = false;
    lobby.getGame(gid)
      .then((d) => {
        if (!cancelled) takenColors = new Set(d.players.map((p) => p.color as string));
      })
      .catch(() => {
        if (!cancelled) takenColors = new Set();
      });
    return () => { cancelled = true; };
  });

  async function join() {
    if (!name.trim() || !gameId.trim()) return;
    joining = true;
    error = '';
    try {
      await onJoin(name.trim(), gameId.trim(), color === '' ? undefined : color);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Errore';
      joining = false;
    }
  }
</script>

<div class="join-screen">
  <h1>🚂 CiufCiuf!</h1>

  <div class="join-form">
    <label for="player-name">Nome giocatore</label>
    <input id="player-name" bind:value={name} placeholder="Il tuo nome" disabled={joining} />

    <label for="game-select">Partita</label>
    {#if loading}
      <p class="muted">Cerco partite in LAN...</p>
    {:else if games.length > 0}
      <select id="game-select" bind:value={gameId} disabled={joining}>
        <option value="">— Scegli una partita —</option>
        {#each games as g}
          {@const started = g.started ? ' 🔒' : ''}
          <option value={g.gameId} disabled={g.started}>
            {g.name} ({g.playersJoined}/{g.maxPlayers}){started}
          </option>
        {/each}
      </select>
    {:else}
      <p class="muted">Nessuna partita in LAN</p>
    {/if}

    <input bind:value={gameId} placeholder="o inserisci Game ID" disabled={joining} />

    <span class="form-label">Colore trenini</span>
    <div class="color-row">
      <button
        class="color-swatch auto"
        class:selected={color === ''}
        onclick={() => (color = '')}
        disabled={joining}
        title="Automatico"
      >?</button>
      {#each ALL_COLORS as c}
        <button
          class="color-swatch"
          class:selected={color === c}
          style="background:{PLAYER_COLOR_CSS[c]}"
          onclick={() => (color = c)}
          disabled={joining || takenColors.has(c)}
          title={takenColors.has(c) ? 'Colore già preso' : c}
        ></button>
      {/each}
    </div>

    {#if error}
      <p class="error">{error}</p>
    {/if}

    <button
      class="btn join-btn"
      onclick={join}
      disabled={joining || !name.trim() || !gameId.trim()}
    >
      {joining ? 'Connessione...' : '🎮 Entra'}
    </button>

    {#if onCancel}
      <button class="btn cancel-btn" onclick={onCancel} disabled={joining}>Indietro</button>
    {/if}
  </div>
</div>

<style>
  .join-screen {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 100vh;
    min-height: 100dvh;
    padding: 1rem;
    background: var(--chrome-bg-0);
    background-image: var(--table-felt);
    color: var(--chrome-text);
    font-family: system-ui, sans-serif;
  }
  h1 {
    font-size: 2rem;
    color: var(--gold);
    margin-bottom: 1.5rem;
    text-shadow: 0 2px 6px rgba(0, 0, 0, 0.5);
  }
  .join-form {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    width: 100%;
    max-width: 320px;
    background: var(--chrome-bg-1);
    border: 1px solid var(--chrome-line);
    padding: 1.5rem;
    border-radius: 14px;
    box-shadow: var(--shadow-board);
  }
  label,
  .form-label {
    font-size: 0.85rem;
    color: var(--gold);
    font-weight: 600;
  }
  input,
  select {
    background: var(--chrome-bg-2);
    border: 1px solid var(--chrome-line);
    color: var(--chrome-text);
    padding: 0.6rem;
    border-radius: 8px;
    font-size: 1rem;
    width: 100%;
    box-sizing: border-box;
  }
  input:focus-visible,
  select:focus-visible {
    outline: 2px solid var(--gold);
    outline-offset: 1px;
  }
  .join-btn {
    background: #1e8449;
    color: #fff;
    padding: 0.75rem;
    font-size: 1.1rem;
  }
  .cancel-btn {
    background: #5a4a35;
    color: #fff;
    padding: 0.5rem;
    font-size: 0.9rem;
  }
  .color-row {
    display: flex;
    gap: 8px;
  }
  .color-swatch {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    border: 3px solid transparent;
    cursor: pointer;
    padding: 0;
    box-shadow:
      0 3px 0 rgba(0, 0, 0, 0.4),
      inset 0 1px 0 rgba(255, 255, 255, 0.25);
    transition: transform 0.06s ease;
  }
  .color-swatch:active:not(:disabled) {
    transform: translateY(2px);
    box-shadow: 0 1px 0 rgba(0, 0, 0, 0.4);
  }
  .color-swatch.selected {
    border-color: var(--gold);
  }
  .color-swatch:disabled {
    opacity: 0.25;
    cursor: not-allowed;
  }
  .color-swatch.auto {
    background: var(--chrome-bg-2);
    color: var(--chrome-text);
    font-weight: bold;
    font-size: 1rem;
  }
  .error {
    color: var(--danger);
    font-size: 0.85rem;
    margin: 0;
  }
  .muted {
    color: var(--chrome-faint);
    font-size: 0.85rem;
    margin: 0;
  }
</style>
