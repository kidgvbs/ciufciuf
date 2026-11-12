<script lang="ts">
  // Schermata di join: nome giocatore + selezione partita (da lobby o manuale).

  import { LobbyClient, loadIdentity, loadPlayerName, type GameListItem } from '../lobby';
  import { createJoinFlow, SLOW_JOIN_MESSAGE, type JoinFlow } from '../join-flow';
  import { PLAYER_COLOR_CSS } from '../colors';
  import type { PlayerColor } from '../proto';

  const ALL_COLORS: PlayerColor[] = ['RED', 'BLUE', 'GREEN', 'YELLOW', 'BLACK'];

  let { host, onJoin, onCancel, notice = '' }: {
    host: string;
    /** `isCancelled()` è vero se l'utente ha premuto "Annulla": l'esito non va applicato. */
    onJoin: (name: string, gameId: string, color: PlayerColor | undefined, isCancelled: () => boolean) => Promise<void>;
    onCancel?: () => void;
    /** Messaggio da mostrare (es. connessione abbandonata); riabilita i controlli. */
    notice?: string;
  } = $props();

  // Nome precompilato dall'ultimo join su questo telefono (localStorage)
  let name = $state(loadPlayerName());
  // Profili salvati sul server (creati al primo join, solo il nome)
  let profiles = $state<string[]>([]);
  // Campo libero per un profilo nuovo (aperto se non ci sono profili)
  let creating = $state(false);
  let gameId = $state('');
  let color = $state<PlayerColor | ''>('');
  let takenColors = $state<Set<string>>(new Set());
  let games = $state<GameListItem[]>([]);
  let loading = $state(false);
  let error = $state('');
  let joining = $state(false);
  let slowJoin = $state(false);

  const lobby = $derived(new LobbyClient(host));

  // localStorage non è reattivo: dopo un join fallito si rilegge l'identità (può essere stata
  // rimossa, es. posto riassegnato dalla TV).
  let identityVersion = $state(0);
  function savedIdentity(gid: string) {
    void identityVersion;
    return loadIdentity(gid);
  }

  // Partita avviata scelta: ci si entra solo con l'identità salvata su questo telefono.
  const rejoinIdentity = $derived.by(() => {
    const gid = gameId.trim();
    const selected = games.find((g) => g.gameId === gid);
    return selected?.started ? savedIdentity(gid) : null;
  });
  // Il server riconosce chi rientra dal nome: quello dell'identità salvata, se noto.
  const effectiveName = $derived((rejoinIdentity?.name ?? name).trim());

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

  $effect(() => {
    lobby.getProfiles()
      .then((p) => {
        profiles = p;
        // Mostra il campo libero se non ci sono profili o se il nome ricordato
        // da questo telefono non è (ancora) tra quelli salvati.
        const known = p.some((x) => x.toLowerCase() === name.trim().toLowerCase());
        if (p.length === 0 || (name.trim() !== '' && !known)) creating = true;
      })
      .catch((e: unknown) => {
        console.warn('[join] profili non disponibili:', e);
        creating = true;
      });
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
      .catch((e: unknown) => {
        console.warn('[join] dettaglio partita non disponibile:', e);
        if (!cancelled) takenColors = new Set();
      });
    return () => { cancelled = true; };
  });

  let joinFlow: JoinFlow | null = null;

  function stopJoinTimer() {
    joinFlow?.stopJoinTimer();
  }

  $effect(() => stopJoinTimer);

  // Connessione abbandonata dal padre: i controlli tornano disponibili.
  $effect(() => {
    if (notice) {
      stopJoinTimer();
      joining = false;
      slowJoin = false;
    }
  });

  // La richiesta non viene mai abortita: se è lenta lo diciamo; "Annulla" la abbandona e la UI
  // ignora l'esito tardivo (l'identità di un join riuscito resta salvata). Altrimenti si prosegue.
  function cancelJoin() {
    joinFlow?.cancel();
    joinFlow = null;
    joining = false;
    slowJoin = false;
    error = '';
  }

  async function join() {
    const playerName = effectiveName;
    const gid = gameId.trim();
    if (!playerName || !gid) return;
    joining = true;
    slowJoin = false;
    error = '';
    stopJoinTimer();
    joinFlow = createJoinFlow({
      attempt: (isCancelled) =>
        onJoin(playerName, gid, rejoinIdentity || color === '' ? undefined : color, isCancelled),
      registeredNames: async () => (await lobby.getGame(gid)).players.map((p) => p.name),
      name: playerName,
      onSlow: () => { slowJoin = true; },
      onError: (message) => {
        identityVersion++;
        error = message;
        joining = false;
        slowJoin = false;
      },
    });
    await joinFlow.run();
  }
</script>

<div class="join-screen">
  <h1>🚂 CiufCiuf!</h1>

  <div class="join-form">
    <span class="form-label">Chi sei?</span>
    {#if rejoinIdentity?.name}
      <p class="rejoin-note">↩ Rientri come <strong>{rejoinIdentity.name}</strong></p>
    {:else}
    {#if profiles.length > 0}
      <div class="profile-list">
        {#each profiles as p (p)}
          <button
            type="button"
            class="profile-chip"
            class:selected={!creating && name.toLowerCase() === p.toLowerCase()}
            onclick={() => { name = p; creating = false; }}
            disabled={joining}
          >{p}</button>
        {/each}
        <button
          type="button"
          class="profile-chip new"
          class:selected={creating}
          onclick={() => { creating = true; name = ''; }}
          disabled={joining}
        >+ Nuovo profilo</button>
      </div>
    {/if}
    {#if creating}
      <input
        id="player-name"
        bind:value={name}
        placeholder="Il tuo nome (lo salviamo per le prossime partite)"
        maxlength="24"
        disabled={joining}
      />
    {/if}
    {/if}

    <label for="game-select">Partita</label>
    {#if loading}
      <p class="muted">Cerco partite in LAN...</p>
    {:else if games.length > 0}
      <select id="game-select" bind:value={gameId} disabled={joining}>
        <option value="">— Scegli una partita —</option>
        {#each games as g}
          {@const rejoinable = g.started && savedIdentity(g.gameId) !== null}
          {@const started = g.started ? (rejoinable ? ' ↩ rientra' : ' 🔒') : ''}
          <option value={g.gameId} disabled={g.started && !rejoinable}>
            {g.name} ({g.playersJoined}/{g.maxPlayers}){started}
          </option>
        {/each}
      </select>
    {:else}
      <p class="muted">Nessuna partita in LAN</p>
    {/if}

    <input bind:value={gameId} placeholder="o inserisci Game ID" disabled={joining} />

    {#if !rejoinIdentity}
      <span class="form-label">Colore trenini</span>
      <div class="color-row">
        <button
          class="color-swatch auto"
          class:selected={color === ''}
          onclick={() => (color = '')}
          disabled={joining}
          title="Automatico"
          aria-label="Colore automatico"
          aria-pressed={color === ''}
        >?</button>
        {#each ALL_COLORS as c}
          <button
            class="color-swatch"
            class:selected={color === c}
            style="background:{PLAYER_COLOR_CSS[c]}"
            onclick={() => (color = c)}
            disabled={joining || takenColors.has(c)}
            title={takenColors.has(c) ? 'Colore già preso' : c}
            aria-label={takenColors.has(c) ? `Colore ${c} già preso` : `Colore ${c}`}
            aria-pressed={color === c}
          ></button>
        {/each}
      </div>
    {/if}

    {#if slowJoin && !error}
      <p class="muted" role="status">{SLOW_JOIN_MESSAGE}</p>
      <button class="btn cancel-btn" onclick={cancelJoin}>Annulla</button>
    {/if}
    {#if error || notice}
      <p class="error" role="alert">{error || notice}</p>
    {/if}

    <button
      class="btn join-btn"
      onclick={join}
      disabled={joining || !effectiveName || !gameId.trim()}
    >
      {joining ? 'Connessione...' : '🎮 Entra'}
    </button>

    {#if onCancel}
      <button class="btn cancel-btn" onclick={() => { cancelJoin(); onCancel(); }} disabled={joining && !slowJoin}>Indietro</button>
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
    flex-wrap: wrap;
    gap: 8px;
  }
  .color-swatch {
    width: 44px;
    height: 44px;
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
  .profile-list {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
  .profile-chip {
    background: var(--chrome-bg-2);
    color: var(--chrome-text);
    border: 2px solid var(--chrome-line);
    border-radius: 999px;
    padding: 0.45rem 0.9rem;
    font-size: 0.95rem;
    font-weight: 600;
    cursor: pointer;
    min-height: 40px;
  }
  .profile-chip.selected {
    border-color: var(--gold);
    background: var(--chrome-bg-1);
  }
  .profile-chip.new {
    border-style: dashed;
    color: var(--chrome-muted);
  }
  .profile-chip:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .rejoin-note {
    margin: 0;
    font-size: 0.95rem;
  }
  .muted {
    color: var(--chrome-faint);
    font-size: 0.85rem;
    margin: 0;
  }
</style>
