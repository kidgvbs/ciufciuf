<script lang="ts">
  // Table view (TV, browser HDMI): mappa condivisa + lobby + game over.
  // Connessione spettatore senza playerId.

  import { onDestroy, onMount, tick, untrack } from 'svelte';
  import { publicStore, connectionStore, applySnapshot, resetStores } from '../lib/stores.svelte.ts';
  import {
    LobbyClient,
    LobbyError,
    saveLastGame,
    loadLastGame,
    clearLastGame,
    parseSeed,
    type LanCandidate,
    type GameDetailPlayer,
    type GameListItem,
    type CreatedGame,
    type SavedGameItem,
    type Standings,
  } from '../lib/lobby';
  import { gameMap } from '../lib/game-map.svelte.ts';
  import { adoptGame } from '../lib/adopt-game';
  import { createGameConnection, type GameConnection, type FatalReason } from '../lib/ws';
  import { currentHostEnv } from '../lib/server-host';
  import { pruneRejoinLinks, reassignSeat } from '../lib/rejoin';
  import { fatalMessage, errorMessage } from '../lib/errors';
  import { qrDataUrl } from '../lib/qr';
  import type { Snapshot, PublicState, ErrorPayload } from '../lib/proto';
  import { PLAYER_COLOR_CSS } from '../lib/colors';
  import { passengerColorCss } from '../lib/passengers';
  import { rankedScores, winnerNames, borrowedRouteLabel, rulesetHasStations } from '../lib/scores';
  import { formatSavedAt } from '../lib/saves';
  import { rulesetCatalog } from '../lib/rulesets.svelte.ts';
  import { defaultRulesetId, groupByMap } from '../lib/rulesets';
  import ModePicker from '../lib/components/ModePicker.svelte';
  import MapSvg from '../lib/components/MapSvg.svelte';
  import MapCard from '../lib/components/MapCard.svelte';
  import TrainCard from '../lib/components/TrainCard.svelte';
  import TrainIcon from '../lib/components/TrainIcon.svelte';
  import ConfirmDialog from '../lib/components/ConfirmDialog.svelte';
  import DeckCard from '../lib/components/DeckCard.svelte';
  import TicketDeckCard from '../lib/components/TicketDeckCard.svelte';
  import TunnelCards from '../lib/components/TunnelCards.svelte';
  import RecentMoves from '../lib/components/RecentMoves.svelte';

  // Il server è sempre quello che serve la pagina (in dev lo raggiunge il proxy di Vite).
  let host = $state<string>(currentHostEnv().origin);
  let gameId = $state<string>('');
  let connection: GameConnection | null = $state(null);
  let hostToken = $state<string>('');
  // Campo del form "Nuova partita": non lo tocca nessun aggancio a partite esistenti.
  let gameNameInput = $state<string>('CiufCiuf!');
  // Nome della partita a cui la TV è agganciata (titolo della lobby).
  let gameTitle = $state<string>('');
  // Creazione in corso: evita il doppio clic su "Crea partita".
  let creating = $state<boolean>(false);
  let seedInput = $state<string>('');
  // Ruleset scelto nel form ('' finché il catalogo del server non è arrivato).
  let rulesetId = $state<string>('');
  let lanCandidates = $state<LanCandidate[]>([]);
  let lanFallback = $state<string>('');
  let lobbyPlayers = $state<GameDetailPlayer[]>([]);
  // Porta del server, letta da /api/lan.
  let lanPort = $state<number | null>(null);
  let availableGames = $state<GameListItem[]>([]);
  let standings = $state<Standings | null>(null);
  // Messaggio d'errore visibile (si chiude da solo).
  let notice = $state<string>('');
  // Troppi spettatori: la connessione è ferma finché l'operatore non preme "Riprova".
  let spectatorBlocked = $state<boolean>(false);
  // Resume: nomi del salvataggio non ancora rientrati e QR di rientro già emessi (nome → URL).
  let missingPlayers = $state<string[]>([]);
  let rejoinLinks = $state<Record<string, string>>({});
  let reassigning = $state<string>('');
  // Dopo --resume il token host è solo nel log del server: l'operatore lo incolla qui.
  let hostTokenInput = $state<string>('');
  const effectiveHostToken = $derived(hostToken || hostTokenInput.trim());
  // Salvataggi riprendibili e id di quello in corso di ripresa (vuoto = nessuno).
  let savedGames = $state<SavedGameItem[]>([]);
  let resumingSave = $state<string>('');
  // Salvataggio di cui si chiede conferma di eliminazione e id di quello in corso di eliminazione.
  let confirmingDelete = $state<SavedGameItem | null>(null);
  let deletingSave = $state<string>('');
  // Titolo "Partite salvate": riceve il focus dopo un'eliminazione riuscita.
  let savedHeading = $state<HTMLHeadingElement | null>();
  // Titolo "Nuova partita": riceve il focus se l'ultimo salvataggio è stato eliminato.
  let newGameHeading = $state<HTMLHeadingElement | null>();
  // Falso dopo lo smontaggio: una risposta tardiva non deve aprire una WebSocket.
  let alive = true;
  onDestroy(() => { alive = false; });
  // Riassegnazione in partita avviata: nome in attesa di conferma (stacca il telefono attuale).
  let confirmingReassign = $state<string | null>(null);
  const saveBusy = $derived(creating || resumingSave !== '' || deletingSave !== '');

  const lobby = $derived(new LobbyClient(host));
  const gameState = $derived(publicStore.state);
  const isLobby = $derived(gameState === 'WAITING_FOR_PLAYERS');
  const isGameOver = $derived(gameState === 'GAME_OVER');
  const isInGame = $derived(!isLobby && !isGameOver && publicStore.gameId !== '');
  const isDisconnected = $derived(connection !== null && !connectionStore.connected);
  // Europe: stazioni residue nel segnapunti solo se la partita le prevede.
  const hasStations = $derived(rulesetHasStations(publicStore.playersSummary, publicStore.stations));
  // Poland: pile delle carte Paese (anche vuote fino a fine partita); Germany 2017: passeggeri.
  const hasCountryCards = $derived(publicStore.countryStacks.length > 0);
  const hasPassengers = $derived(
    publicStore.passengers.length > 0 || publicStore.playersSummary.some((p) => p.passengers.length > 0),
  );
  const cityNames = $derived(new Map((gameMap.state.data?.cities ?? []).map((c) => [c.id, c.name])));
  function cityName(id: string): string {
    return cityNames.get(id) ?? id;
  }
  // Europe: in TUNNEL_RESOLUTION le carte rivelate sono pubbliche; la TV le mostra a tutti.
  const tunnelReveal = $derived(gameState === 'TUNNEL_RESOLUTION' ? publicStore.tunnelReveal : null);
  const tunnelRoute = $derived(
    tunnelReveal ? (gameMap.state.data?.routes.find((r) => r.id === tunnelReveal.route_id) ?? null) : null,
  );
  const tunnelPlayer = $derived(
    publicStore.playersSummary.find((p) => p.player_id === publicStore.currentTurnPlayerId) ?? null,
  );
  function mapCityName(id: string): string {
    return gameMap.state.data?.cities.find((c) => c.id === id)?.name ?? id;
  }

  function describe(e: unknown): string {
    return e instanceof Error ? e.message : String(e);
  }

  $effect(() => {
    if (!connectionStore.lastError) return;
    const t = setTimeout(() => { connectionStore.lastError = null; }, 8000);
    return () => clearTimeout(t);
  });

  $effect(() => {
    if (!notice) return;
    const t = setTimeout(() => { notice = ''; }, 8000);
    return () => clearTimeout(t);
  });

  async function loadLan() {
    try {
      const info = await lobby.getLan();
      lanCandidates = info.candidates; lanFallback = info.fallback; lanPort = info.port;
    } catch (e) {
      console.warn('[tv] /api/lan non disponibile:', e);
      notice = `Indirizzi LAN non disponibili: ${describe(e)}`;
    }
  }

  async function createGame() {
    if (creating) return;
    creating = true;
    try {
      // Il seed è un attrezzo di sviluppo: in produzione il campo non esiste e il server lo rifiuta.
      const seed = import.meta.env.DEV ? parseSeed(seedInput) : undefined;
      if (seed === null) {
        notice = 'Seed non valido: serve un intero non negativo';
        return;
      }
      const name = gameNameInput;
      const created = await lobby.createGame(rulesetId, name, seed);
      gameTitle = name;
      await adoptHostedGame(created);
    } catch (e) {
      console.error('Creazione fallita:', e);
      notice = `Creazione partita fallita: ${describe(e)}`;
    } finally {
      creating = false;
    }
  }

  // Riprende un salvataggio dalla TV: l'hostToken arriva dal server, senza passare dal log.
  async function resumeSave(save: SavedGameItem) {
    if (saveBusy) return;
    resumingSave = save.gameId;
    try {
      const resumed = await lobby.resumeSave(save.gameId);
      gameTitle = save.name;
      await adoptHostedGame(resumed);
    } catch (e) {
      console.error('Ripresa fallita:', e);
      // L'elenco può essere vecchio (partita già ripresa o salvataggio sparito): prima
      // si aggiorna, poi si mostra l'errore, così non viene coperto.
      await refreshGames();
      notice = `Ripresa di ${save.name} fallita: ${describe(e)}`;
    } finally {
      resumingSave = '';
    }
  }

  // Conferma del dialog: elimina il salvataggio scelto e ricarica l'elenco.
  async function confirmDelete() {
    const save = confirmingDelete;
    confirmingDelete = null;
    if (!save || saveBusy) return;
    deletingSave = save.gameId;
    try {
      await lobby.deleteSave(save.gameId);
      await refreshGames();
      // Il bottone premuto non esiste più: il focus passa al titolo dell'elenco.
      await tick();
      (savedHeading?.isConnected ? savedHeading : newGameHeading)?.focus();
    } catch (e) {
      console.error('Eliminazione fallita:', e);
      // Come per la ripresa: prima si aggiorna l'elenco, poi si mostra l'errore.
      await refreshGames();
      notice = `Eliminazione di ${save.name} fallita: ${describe(e)}`;
    } finally {
      deletingSave = '';
    }
  }

  // La TV diventa host della partita appena creata o ripresa.
  async function adoptHostedGame(result: CreatedGame) {
    await adoptGame({
      isAlive: () => alive,
      adopt: () => {
        gameId = result.gameId; hostToken = result.hostToken;
        // Ricorda l'ultima partita: una TV ricaricata si riaggancia da sola
        // (hostToken serve a riprendere il bottone Start dopo un reload in lobby).
        saveLastGame({ gameId: result.gameId, hostToken: result.hostToken });
        updateUrlGame(result.gameId);
      },
      refreshLobby,
      connect: connectSpectator,
    });
  }

  async function refreshLobby() {
    if (!gameId) return;
    try {
      const d = await lobby.getGame(gameId);
      lobbyPlayers = d.players;
      missingPlayers = d.missingPlayers ?? [];
    } catch (e) {
      console.warn('[tv] aggiornamento lobby fallito:', e);
      notice = `Aggiornamento lobby fallito: ${describe(e)}`;
    }
  }

  // Le tre letture sono indipendenti: ognuna aggiorna il suo elenco e, se fallisce, lo dice.
  async function refreshGames() {
    const [games, ranking, saves] = await Promise.allSettled([
      lobby.listGames(),
      lobby.getStandings(),
      lobby.listSaves(),
    ]);
    const failures: string[] = [];
    if (games.status === 'fulfilled') availableGames = games.value;
    else {
      console.warn('[tv] elenco partite non disponibile:', games.reason);
      failures.push(`partite (${describe(games.reason)})`);
    }
    if (ranking.status === 'fulfilled') standings = ranking.value;
    else {
      console.warn('[tv] classifica non disponibile:', ranking.reason);
      failures.push(`classifica (${describe(ranking.reason)})`);
    }
    if (saves.status === 'fulfilled') savedGames = saves.value;
    else {
      console.warn('[tv] salvataggi non disponibili:', saves.reason);
      failures.push(`salvataggi (${describe(saves.reason)})`);
    }
    if (failures.length > 0) notice = `Impossibile leggere: ${failures.join('; ')}`;
  }

  async function startGame() {
    if (!gameId || !hostToken) return;
    try {
      await lobby.startGame(gameId, hostToken);
    } catch (e) {
      console.error('Start:', e);
      notice = `Avvio partita fallito: ${describe(e)}`;
    }
  }

  // Resume: rilascia una nuova identità per un nome salvato e la mostra come QR per il telefono.
  async function reassign(name: string) {
    const token = effectiveHostToken;
    const gid = gameId;
    if (!gid || !token || reassigning) return;
    reassigning = name;
    try {
      const url = await reassignSeat({
        gameId: gid,
        token,
        name,
        controllerBase,
        request: (g, t, n) => lobby.reassignPlayer(g, t, n),
        currentGameId: () => gameId,
      });
      // La TV ha lasciato o cambiato partita durante l'attesa: l'esito non le appartiene.
      if (url === null) return;
      rejoinLinks = { ...rejoinLinks, [name]: url };
      // Token digitato dall'operatore e accettato dal server: la TV lo adotta (abilita anche Start).
      if (!hostToken) {
        hostToken = token;
        hostTokenInput = '';
        saveLastGame({ gameId: gid, hostToken });
      }
    } catch (e) {
      console.error('Riassegnazione:', e);
      if (gameId === gid) notice = `Riassegnazione di ${name} fallita: ${describe(e)}`;
    } finally {
      reassigning = '';
    }
  }

  // Aggancia la TV a una partita esistente (lista "Partite in LAN").
  function followGame(g: GameListItem) {
    const saved = loadLastGame();
    gameId = g.gameId;
    hostToken = saved && saved.gameId === g.gameId ? saved.hostToken : '';
    gameTitle = g.name;
    saveLastGame({ gameId: g.gameId, hostToken });
    updateUrlGame(g.gameId);
    void refreshLobby();
    connectSpectator();
  }

  // Stacca la TV dalla partita corrente e torna alla lista; `forget` cancella anche
  // l'ultima partita ricordata.
  function leaveGame(forget = true) {
    if (connection) { connection.close(); connection = null; }
    spectatorBlocked = false;
    if (forget) clearLastGame();
    updateUrlGame('');
    gameId = '';
    gameTitle = '';
    hostToken = '';
    lobbyPlayers = [];
    missingPlayers = [];
    rejoinLinks = {};
    resetStores();
    void refreshGames();
  }

  function retrySpectator() {
    spectatorBlocked = false;
    connectionStore.lastError = null;
    connectSpectator();
  }

  function onFatal(reason: FatalReason) {
    if (reason === 'TOO_MANY_SPECTATORS') {
      // La partita esiste ancora: niente leaveGame, solo un messaggio con "Riprova".
      connection = null;
      connectionStore.connected = false;
      connectionStore.lastError = null;
      spectatorBlocked = true;
      return;
    }
    // Una versione incompatibile non invalida la partita: basta ricaricare la pagina.
    leaveGame(reason !== 'PROTOCOL_VERSION');
    notice = fatalMessage(reason);
  }

  // Sincronizza ?game=<id> nell'URL (così un reload si riaggancia da solo).
  function updateUrlGame(id: string) {
    const params = new URLSearchParams(window.location.search);
    if (id) params.set('game', id); else params.delete('game');
    const qs = params.toString();
    window.history.replaceState({}, '', window.location.pathname + (qs ? `?${qs}` : ''));
  }

  // Zoom della mappa sulla tratta appena occupata: solo un PUBLIC_STATE in tempo reale che aggiunge
  // una tratta lo scatena (non gli SNAPSHOT di connessione/riconnessione, non l'ultima mossa che
  // chiude la partita, non il primo stato di una partita).
  let claimFocus = $state<{ seq: number; routeId: string } | null>(null);
  let claimSeq = 0;
  function applyLivePublicState(state: PublicState) {
    const wasPlaying = publicStore.gameId === state.game_id && !isLobby && !isGameOver;
    const known = wasPlaying ? new Set(publicStore.claimedRoutes.map((cr) => cr.route_id)) : null;
    publicStore.apply(state);
    if (!known || state.state === 'GAME_OVER') return;
    const added = state.claimed_routes.filter((cr) => !known.has(cr.route_id));
    const last = added[added.length - 1];
    if (last) claimFocus = { seq: ++claimSeq, routeId: last.route_id };
  }

  function connectSpectator() {
    if (!alive || !gameId) return;
    if (connection) connection.close();
    connection = createGameConnection({
      host, gameId,
      callbacks: {
        onSnapshot(snap: Snapshot) {
          applySnapshot(snap);
        },
        onPublicState: applyLivePublicState,
        onPrivateState() {},
        onError(err: ErrorPayload) { connectionStore.lastError = err; },
        onConnectionChange(connected: boolean) { connectionStore.connected = connected; },
        onFatal,
      },
    });
  }

  // Attach iniziale (una sola volta, fuori dalla reattività):
  // priorità ?game=<id> > localStorage ciufciuf-last-game > lista partite.
  onMount(() => {
    void loadLan();
    rulesetCatalog.load(host);
    void (async () => {
      const params = new URLSearchParams(window.location.search);
      const paramGame = params.get('game');
      const saved = loadLastGame();
      const candidate = paramGame ?? saved?.gameId ?? '';
      if (candidate) {
        try {
          const detail = await lobby.getGame(candidate);
          gameId = candidate;
          hostToken = saved && saved.gameId === candidate ? saved.hostToken : '';
          gameTitle = detail.name;
          lobbyPlayers = detail.players;
          missingPlayers = detail.missingPlayers ?? [];
          updateUrlGame(candidate);
          connectSpectator();
          return;
        } catch (e) {
          if (e instanceof LobbyError && e.status === 404) {
            // Partita non più esistente (es. server riavviato): dimenticala
            if (saved?.gameId === candidate) clearLastGame();
            if (paramGame) updateUrlGame('');
          } else {
            console.warn('[tv] aggancio alla partita fallito:', e);
            notice = `Impossibile agganciarsi alla partita: ${describe(e)}`;
          }
        }
      }
      await refreshGames();
    })();
  });

  // Mappa e biglietti della partita agganciata, dal ruleset indicato dal server.
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
  $effect(() => () => { connection?.close(); connection = null; });

  // Nome del bonus a fine partita: se il catalogo non era arrivato, riprova.
  $effect(() => {
    if (isGameOver) untrack(() => rulesetCatalog.load(host));
  });

  // Preselezione del ruleset: appena arriva il catalogo, o se quello scelto non c'è più.
  $effect(() => {
    const catalog = rulesetCatalog.items;
    if (!catalog.some((r) => r.id === rulesetId)) rulesetId = defaultRulesetId(catalog) ?? '';
  });

  // Selettore «Nuova partita»: una card per mappa, le modalità della mappa scelta sotto le card.
  const mapGroups = $derived(groupByMap(rulesetCatalog.items));
  const selectedMapId = $derived(rulesetCatalog.items.find((r) => r.id === rulesetId)?.mapId ?? '');
  const selectedGroup = $derived(mapGroups.find((g) => g.mapId === selectedMapId));

  // Colonne e righe di fine partita: solo i bonus che il ruleset prevede.
  const showLongestPath = $derived(rulesetCatalog.hasLongestPath(gameMap.rulesetId));
  const showGlobetrotter = $derived(rulesetCatalog.hasGlobetrotter(gameMap.rulesetId));
  const globetrotterIds = $derived(
    (publicStore.finalScores ?? []).filter((fs) => fs.globetrotterBonus > 0).map((fs) => fs.playerId),
  );
  function namesOf(ids: string[]): string {
    return ids.map((pid) => publicStore.playersSummary.find((ps) => ps.player_id === pid)?.name ?? pid).join(', ');
  }

  // QR di rientro: la voce di un giocatore sparisce quando torna connesso o esce dal roster.
  $effect(() => {
    const pruned = pruneRejoinLinks(rejoinLinks, publicStore.playersSummary, missingPlayers);
    if (pruned !== rejoinLinks) rejoinLinks = pruned;
  });

  // Helpers
  function isCurrentPlayer(pid: string) { return publicStore.currentTurnPlayerId === pid; }

  // Presence: giocatori senza WebSocket attiva, solo a
  // partita avviata (in lobby il QR c'è già; a game over non serve).
  // Il QR di rientro per-giocatore (Riassegna) sostituisce quello generico: chi ce l'ha
  // sotto la propria scheda non compare anche nella striscia generica.
  const offlinePlayers = $derived(
    isInGame
      ? publicStore.playersSummary.filter((p) => !p.connected && !(hostToken && rejoinLinks[p.name]))
      : [],
  );
  const offlineNames = $derived(offlinePlayers.map((p) => p.name).join(', '));

  // Base URL del controller sulla LAN: stesso della lobby.
  const controllerBase = $derived.by(() => {
    const cand = lanCandidates.find((c) => c.preferred) ?? lanCandidates.at(0);
    return cand?.url ?? (lanFallback || host);
  });

  // URL di rientro per il QR dei giocatori offline.
  const rejoinUrl = $derived(controllerBase ? controllerBase.replace(/\/$/, '') + '/controller' : '');

  // Lista giocatori in lobby: preferisci lo stato live dal WS (broadcast su ogni
  // join), con fallback sul polling REST finché il primo PUBLIC_STATE non arriva.
  const lobbyList = $derived(
    publicStore.playersSummary.length > 0
      ? publicStore.playersSummary.map((p) => ({ key: p.player_id, name: p.name, color: p.color }))
      : lobbyPlayers.map((p) => ({ key: p.playerId, name: p.name, color: p.color })),
  );

  // Polling lista giocatori in lobby
  $effect(() => {
    if (isLobby && gameId) {
      const interval = setInterval(refreshLobby, 3000);
      return () => clearInterval(interval);
    }
  });
</script>

{#snippet rejoinAction(name: string, confirm: boolean)}
  <button
    class="btn"
    onclick={() => (confirm ? (confirmingReassign = name) : reassign(name))}
    disabled={reassigning !== '' || !effectiveHostToken}
  >{rejoinLinks[name] ? 'Riassegna di nuovo' : 'Riassegna'}</button>
  {#if rejoinLinks[name]}
    <img
      class="missing-qr"
      src={qrDataUrl(rejoinLinks[name])}
      alt="QR per {name}"
    />
  {/if}
{/snippet}

<div class="tv-root">
  <div class="tv-banners">
    {#if isDisconnected}
      <div class="tv-banner" role="status" aria-live="polite">
        ⚠️ Disconnesso dal server &mdash; riconnessione in corso...
      </div>
    {/if}
    {#if spectatorBlocked}
      <div class="tv-banner" role="alert">
        ⚠️ {fatalMessage('TOO_MANY_SPECTATORS')}
        <button class="retry-btn" onclick={retrySpectator}>Riprova</button>
      </div>
    {/if}
    {#if connectionStore.lastError}
      <div role="alert">
        <button class="tv-banner" onclick={() => (connectionStore.lastError = null)}>
          ❌ {errorMessage(connectionStore.lastError)}
        </button>
      </div>
    {/if}
    {#if notice}
      <div role="alert">
        <button class="tv-banner" onclick={() => (notice = '')}>❌ {notice}</button>
      </div>
    {/if}
  </div>
  {#if isLobby}
    <div class="lobby">
      <h1>🚂 CiufCiuf!</h1>
      <div class="lobby-layout">
        <div class="lobby-left">
          <h2>Connettiti</h2>
          {#if lanCandidates.length > 0}
            <div class="qr-list">
              {#each lanCandidates as cand}
                {@const controllerUrl = cand.url + '/controller'}
                <div class="qr-item" class:preferred={cand.preferred}>
                  <img src={qrDataUrl(controllerUrl)} alt="QR {cand.ip}" class="qr-img" />
                  <div class="qr-label">{cand.ip}{lanPort !== null ? `:${lanPort}` : ''}{cand.preferred ? ' ⭐' : ''}</div>
                </div>
              {/each}
            </div>
          {:else}
            <div class="qr-fallback"><p>Apri sul telefono:</p><code>{lanFallback}/controller</code></div>
          {/if}
        </div>
        <div class="lobby-right">
          {#if !gameId}
            <div class="create-game">
              <h2 tabindex="-1" bind:this={newGameHeading}>Nuova partita</h2>
              <div class="map-cards">
                {#each mapGroups as group (group.mapId)}
                  {@const standard = group.rulesets[0]!}
                  <MapCard
                    {host}
                    ruleset={standard}
                    selected={selectedMapId === group.mapId}
                    onSelect={() => {
                      if (selectedMapId !== group.mapId) rulesetId = standard.id;
                    }}
                  />
                {/each}
              </div>
              {#if selectedGroup}
                <ModePicker modes={selectedGroup.rulesets} value={rulesetId} onSelect={(id) => (rulesetId = id)} />
              {/if}
              {#if rulesetCatalog.status === 'loading'}
                <p class="hint" role="status">Caricamento mappe…</p>
              {:else if rulesetCatalog.status === 'error'}
                <p class="hint" role="alert">
                  Mappe non disponibili: {rulesetCatalog.error}
                  <button class="btn retry-btn" onclick={() => rulesetCatalog.load(host)}>Riprova</button>
                </p>
              {:else if rulesetCatalog.status === 'ready' && rulesetCatalog.items.length === 0}
                <p class="hint">Nessuna mappa disponibile</p>
              {/if}
              <label>Nome: <input bind:value={gameNameInput} /></label>
              {#if import.meta.env.DEV}
                <label>Seed (solo sviluppo — partita ripetibile): <input bind:value={seedInput} inputmode="numeric" placeholder="casuale" /></label>
              {/if}
              <button class="btn" onclick={createGame} disabled={creating || resumingSave !== '' || rulesetId === ''}>Crea partita</button>
            </div>
            {#if availableGames.length > 0}
              <div class="existing-games">
                <h2>Partite in LAN</h2>
                {#each availableGames as g (g.gameId)}
                  <div class="game-row">
                    <div class="game-row-info">
                      <strong>{g.name}</strong>
                      <span class="game-meta">
                        {rulesetCatalog.title(g.rulesetId) ?? g.rulesetId} · {g.playersJoined}/{g.maxPlayers} giocatori
                        · {g.started ? 'in corso' : 'in lobby'}
                      </span>
                    </div>
                    <button class="btn follow-btn" onclick={() => followGame(g)} disabled={saveBusy}>Segui</button>
                  </div>
                {/each}
              </div>
            {/if}
            {#if savedGames.length > 0}
              <div class="existing-games saved-games">
                <h2 tabindex="-1" bind:this={savedHeading}>Partite salvate</h2>
                {#each savedGames as s (s.gameId)}
                  <div class="game-row">
                    <div class="game-row-info">
                      <strong>{s.name}</strong>
                      <span class="game-meta">
                        {#if rulesetCatalog.title(s.rulesetId)}{rulesetCatalog.title(s.rulesetId)} · {/if}{s.players.join(', ')} · {formatSavedAt(s.updatedAt)}
                      </span>
                    </div>
                    <div class="game-row-actions">
                      <button
                        class="btn follow-btn"
                        onclick={() => resumeSave(s)}
                        disabled={saveBusy}
                      >Riprendi</button>
                      <button
                        class="btn delete-btn"
                        onclick={() => (confirmingDelete = s)}
                        disabled={saveBusy}
                      >Elimina</button>
                    </div>
                  </div>
                {/each}
              </div>
            {/if}
          {:else}
            <div class="game-info">
              <h2>{gameTitle}</h2>
              <p>Giocatori: {lobbyList.length}</p>
              <div class="player-list">
                {#each lobbyList as p (p.key)}
                  <div class="player-chip" style="border-color:{PLAYER_COLOR_CSS[p.color]}">
                    <span class="chip-dot" style="background:{PLAYER_COLOR_CSS[p.color]}"></span>
                    <span class="chip-name">{p.name}</span>
                  </div>
                {/each}
              </div>
              {#if hostToken && lobbyList.length >= 2}
                <button class="btn start-btn" onclick={startGame}>🚀 Start!</button>
              {:else if hostToken}
                <p class="hint">Minimo 2 giocatori</p>
              {:else}
                <p class="hint">Questa TV non ha il token host e non può avviare la partita.</p>
                <button class="btn" onclick={() => leaveGame()}>➕ Crea una nuova partita</button>
              {/if}
              {#if missingPlayers.length > 0}
                <div class="missing-players">
                  <h3>In attesa di rientro</h3>
                  {#each missingPlayers as missing (missing)}
                    <div class="missing-row">
                      <span>{missing}</span>
                      {@render rejoinAction(missing, false)}
                    </div>
                  {/each}
                  {#if !hostToken}
                    <label class="missing-hint">
                      Token host (solo dopo <code>--resume</code> da riga di comando, dal log del server):
                      <input bind:value={hostTokenInput} autocomplete="off" spellcheck="false" />
                    </label>
                  {/if}
                </div>
              {/if}
              <button class="btn leave-game-btn" onclick={() => leaveGame()}>✕ Cambia partita</button>
            </div>
          {/if}

          <!-- Classifica generale: visibile prima di ogni partita -->
          <div class="existing-games standings">
            <h2>🏆 Classifica generale</h2>
            {#if standings && standings.standings.length > 0}
              <p class="game-meta">{standings.games} partite concluse</p>
              <table>
                <thead>
                  <tr><th>#</th><th>Giocatore</th><th>Vittorie</th><th>Partite</th><th>Punti</th><th>Record</th></tr>
                </thead>
                <tbody>
                  {#each standings.standings as row, i (row.name)}
                    <tr class:winner={i === 0}>
                      <td>{i + 1}</td>
                      <td>{row.name}</td>
                      <td>{row.wins}</td>
                      <td>{row.games}</td>
                      <td>{row.totalPoints}</td>
                      <td>{row.best}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {:else}
              <p class="game-meta">Ancora nessuna partita conclusa: la prima fa classifica!</p>
            {/if}
          </div>
        </div>
      </div>
    </div>

  {:else if isGameOver}
    <div class="game-over">
      <h1>🏁 Fine partita!</h1>
      {#if publicStore.finalScores}
        {@const winners = winnerNames(publicStore.finalScores, publicStore.playersSummary)}
        {#if winners}<div class="winner-line">🥇 Vince {winners}!</div>{/if}
        <table>
          <thead><tr><th>Giocatore</th><th>Rotte</th><th>Biglietti +</th><th>Biglietti −</th>{#if showLongestPath}<th>🏆 {rulesetCatalog.longestPathLabel(gameMap.rulesetId)}</th>{/if}{#if showGlobetrotter}<th>🌍 {rulesetCatalog.globetrotterLabel(gameMap.rulesetId)}</th>{/if}{#if hasStations}<th>Stazioni</th>{/if}{#if hasCountryCards}<th>Carte Paese</th>{/if}{#if hasPassengers}<th>Passeggeri</th>{/if}<th>Totale</th></tr></thead>
          <tbody>
            {#each rankedScores(publicStore.finalScores) as fs}
              {@const p = publicStore.playersSummary.find(ps => ps.player_id === fs.playerId)}
              <tr class:winner={fs.winner}>
                <td><span class="chip-dot" style="background:{p ? PLAYER_COLOR_CSS[p.color] : '#666'}"></span> {fs.winner ? '🥇 ' : ''}{p?.name ?? fs.playerId}</td>
                <td>{fs.routePoints}</td><td>+{fs.ticketCompleted}</td>
                <td>{fs.ticketFailed < 0 ? fs.ticketFailed : '0'}</td>
                {#if showLongestPath}<td>{fs.longestPathBonus > 0 ? `+${fs.longestPathBonus}` : '0'}</td>{/if}
                {#if showGlobetrotter}<td>{fs.globetrotterBonus > 0 ? `🌍 +${fs.globetrotterBonus}` : '0'}</td>{/if}
                {#if hasStations}<td>{fs.stationPoints > 0 ? `+${fs.stationPoints}` : '0'}</td>{/if}
                {#if hasCountryCards}<td>{fs.countryPoints > 0 ? `+${fs.countryPoints}` : '0'}</td>{/if}
                {#if hasPassengers}<td>{fs.passengerPoints > 0 ? `+${fs.passengerPoints}` : '0'}</td>{/if}
                <td class="total">{fs.total}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        {#if publicStore.finalScores.some((fs) => fs.borrowedRoutes.length > 0)}
          <div class="borrowed-routes">
            <h3>Rotte in prestito (stazioni)</h3>
            {#each rankedScores(publicStore.finalScores) as fs}
              {#if fs.borrowedRoutes.length > 0}
                {@const p = publicStore.playersSummary.find(ps => ps.player_id === fs.playerId)}
                <div class="borrowed-row">
                  <span class="chip-dot" style="background:{p ? PLAYER_COLOR_CSS[p.color] : '#666'}"></span>
                  <strong>{p?.name ?? fs.playerId}</strong>:
                  {fs.borrowedRoutes.map((b) => borrowedRouteLabel(b, gameMap.state.data)).join(', ')}
                </div>
              {/if}
            {/each}
          </div>
        {/if}
        {#if showLongestPath && publicStore.longestPathPlayerIds}
          <div class="longest-path">🏆 {rulesetCatalog.longestPathLabel(gameMap.rulesetId)}: {namesOf(publicStore.longestPathPlayerIds)}</div>
        {/if}
        {#if showGlobetrotter && globetrotterIds.length > 0}
          <div class="longest-path">🌍 {rulesetCatalog.globetrotterLabel(gameMap.rulesetId)}: {namesOf(globetrotterIds)}</div>
        {/if}
      {/if}
      <!-- Mappa a fine partita: il percorso più lungo è evidenziato (glow dorato). -->
      <div class="gameover-map"><MapSvg /></div>
      <!-- Via d'uscita dalla schermata di fine partita
           (l'attach automatico riaggancia questa partita a ogni reload). -->
      <button class="btn leave-game-btn" onclick={() => leaveGame()}>✕ Cambia partita</button>
    </div>

  {:else if isInGame}
    <div class="game-layout">
      <div class="sidebar">
        <div class="sidebar-head">
          <h2>Segnapunti</h2>
          <TicketDeckCard count={publicStore.ticketDeckSize} />
        </div>
        {#each publicStore.playersSummary as p}
          <div class="player-card" class:current-turn={isCurrentPlayer(p.player_id)}
            class:offline={!p.connected}
            style="border-left:4px solid {PLAYER_COLOR_CSS[p.color]}">
            <div class="player-name">
              <span class="pawn" style="--pawn:{PLAYER_COLOR_CSS[p.color]}">
                <TrainIcon color="RED" fill={PLAYER_COLOR_CSS[p.color]} uid="tv-{p.player_id.slice(0, 6)}" />
              </span>
              <span class="chip-name">{p.name}</span>
              {#if !p.connected}<span class="offline-tag">⚡ offline</span>{/if}
              {#if isCurrentPlayer(p.player_id)}<span class="turn-tag">turno</span>{/if}
            </div>
            <div class="score-line"><span class="score-num">{p.score}</span><span class="score-label">punti</span></div>
            <div class="player-stats"><span>🚂 {p.trains_remaining}</span>{#if hasStations}<span title="Stazioni rimaste">🏠 {p.stations_remaining}</span>{/if}<span>🃏 {p.cards_hand_count}</span><span>🎫 {p.tickets_count}</span></div>
            {#if hasPassengers && p.passengers.length > 0}
              <div class="player-extras" title="Passeggeri raccolti">
                {#each p.passengers as pc (pc.color)}
                  <span class="passenger-chip"><span class="passenger-dot" style="background:{passengerColorCss(pc.color)}"></span>{pc.count}</span>
                {/each}
              </div>
            {/if}
            {#if hasCountryCards && p.country_cards.length > 0}
              <div class="player-extras" title="Carte Paese prese">
                {#each p.country_cards as cc (cc.country_id)}
                  <span>{cityName(cc.country_id)} {cc.values.join('+')}</span>
                {/each}
              </div>
            {/if}
            {#if hostToken && !p.connected}
              <div class="rejoin-action">{@render rejoinAction(p.name, true)}</div>
            {/if}
          </div>
        {/each}
        <!-- QR di rientro: qualcuno è caduto, il telefono ritrova la partita
             da solo (identità in localStorage). Area dedicata in plancia,
             non copre la mappa. -->
        {#if offlinePlayers.length > 0}
          <div class="offline-strip" role="status" aria-live="polite">
            <div class="offline-strip-text">
              <strong>{offlineNames}</strong>
              {offlinePlayers.length === 1 ? 'è offline' : 'sono offline'}
              <span class="offline-sub">QR per rientrare — la partita si riprende da sola</span>
            </div>
            {#if rejoinUrl}
              <img class="offline-qr" src={qrDataUrl(rejoinUrl)} alt="QR per rientrare in partita" />
            {/if}
          </div>
        {/if}
        <RecentMoves
          moves={publicStore.recentMoves}
          players={publicStore.playersSummary}
          map={gameMap.state.data}
        />
        <div class="visible-deck">
          <h3>Carte scoperte</h3>
          <div class="cards-row">
            {#each publicStore.visibleDeck as card}
              {#if card}
                <div class="face-up-cell">
                  <TrainCard color={card} small />
                </div>
              {:else}
                <div class="mini-card empty">—</div>
              {/if}
            {/each}
            <!-- Mazzo coperto con conteggio (deck_size dal server) -->
            <div class="face-up-cell deck-cell">
              <DeckCard count={publicStore.deckSize} small />
            </div>
          </div>
        </div>
        {#if hasCountryCards}
          <div class="country-stacks">
            <h3>Carte Paese</h3>
            {#each publicStore.countryStacks as stack (stack.country_id)}
              <div class="country-stack">
                <span>{cityName(stack.country_id)}</span>
                <span>{stack.values.length > 0 ? `in cima ${stack.values[0]} · ${stack.values.length} rimaste` : 'esaurite'}</span>
              </div>
            {/each}
          </div>
        {/if}
        {#if publicStore.lastRound}
          <div class="last-round-banner">⚠️ ULTIMO GIRO</div>
        {/if}
      </div>
      <MapSvg focusClaim={claimFocus} />
      {#if tunnelReveal}
        <div class="tunnel-banner" role="status" aria-live="polite">
          <h2>
            🚇 Tunnel{tunnelRoute ? `: ${mapCityName(tunnelRoute.from)} → ${mapCityName(tunnelRoute.to)}` : ''}
          </h2>
          <p class="tunnel-who">
            {tunnelPlayer?.name ?? '…'} decide se pagare {tunnelReveal.extra_cost}
            {tunnelReveal.extra_cost === 1 ? 'carta extra' : 'carte extra'}
          </p>
          <TunnelCards reveal={tunnelReveal} />
        </div>
      {/if}
    </div>
  {/if}

  {#if confirmingReassign}
    <ConfirmDialog
      title="Riassegnare il posto?"
      message={`Riassegnare il posto di ${confirmingReassign}? Il suo telefono attuale verrà scollegato.`}
      confirmLabel="Riassegna"
      onConfirm={() => {
        const name = confirmingReassign;
        confirmingReassign = null;
        if (name) void reassign(name);
      }}
      onCancel={() => (confirmingReassign = null)}
    />
  {/if}
  {#if confirmingDelete}
    <ConfirmDialog
      title="Eliminare la partita salvata?"
      message={`Eliminare la partita salvata «${confirmingDelete.name}»? Il file resta in saves/ come backup.`}
      confirmLabel="Elimina"
      onConfirm={confirmDelete}
      onCancel={() => (confirmingDelete = null)}
    />
  {/if}
</div>

<style>
  /* ── Tavolo (TV): legno caldo scuro, texture procedurale ─────────── */
  .tv-root {
    width:100vw; height:100vh; overflow:hidden;
    background: var(--chrome-bg-0);
    background-image: var(--table-felt);
    color: var(--chrome-text);
    font-family: system-ui, sans-serif;
  }
  h1 { font-size:2.5rem; color:var(--gold); margin:0; text-shadow:0 2px 6px rgba(0,0,0,0.5); }
  h2 { font-size:1.3rem; color:var(--gold); margin:0 0 0.5rem; }
  h3 { font-size:1rem; color:var(--gold); margin:0.5rem 0; }

  /* Lobby */
  .lobby { padding:2rem; height:100%; overflow-y:auto; box-sizing:border-box; }
  .lobby-layout { display:flex; gap:2rem; margin-top:1.5rem; }
  .lobby-left { flex:1; }
  .lobby-right { flex:1; }
  .qr-list { display:flex; gap:1rem; flex-wrap:wrap; }
  .qr-item {
    text-align:center; padding:0.5rem;
    background: var(--chrome-bg-1);
    border:2px solid var(--chrome-line); border-radius:12px;
    box-shadow: var(--shadow-panel);
  }
  .qr-item.preferred { border-color:var(--gold); }
  .qr-img { width:140px; height:140px; border-radius:6px; background:#fff; }
  .qr-label { font-size:0.8rem; margin-top:0.25rem; color:var(--chrome-muted); }
  .qr-fallback { padding:1rem; }
  .qr-fallback code { background:var(--chrome-bg-2); border:1px solid var(--chrome-line); padding:0.5rem 1rem; border-radius:6px; font-size:1.2rem; }

  .create-game, .existing-games {
    background: var(--chrome-bg-1);
    border:1px solid var(--chrome-line);
    padding:1rem; border-radius:12px;
    box-shadow: var(--shadow-panel);
  }
  .existing-games { margin-top:1rem; }
  .standings table { margin-top:0.5rem; }
  .create-game label { display:block; margin:0.5rem 0; font-size:0.9rem; }
  .create-game input {
    margin-left:0.5rem; background:var(--chrome-bg-2); border:1px solid var(--chrome-line);
    color:var(--chrome-text); padding:0.3rem 0.5rem; border-radius:6px;
  }
  .map-cards { display:flex; gap:1rem; flex-wrap:wrap; margin:0.75rem 0; }
  .game-row { display:flex; align-items:center; justify-content:space-between; gap:1rem; padding:0.5rem 0; border-bottom:1px solid var(--chrome-line); }
  .game-row:last-child { border-bottom:none; }
  .game-row-info { display:flex; flex-direction:column; gap:0.15rem; }
  .game-meta { font-size:0.8rem; color:var(--chrome-muted); }
  .game-row-actions { display:flex; gap:0.5rem; }
  /* Contrasto 4.5:1 con testo #fff */
  button { font-size:1rem; margin-top:0.5rem; padding:0.5rem 1rem; color:#fff; background:#9a5a16; border-radius:var(--btn-radius); }
  button:not(.btn) { border:none; cursor:pointer; }
  button:not(.btn):hover { background:#cf8a2a; }
  .follow-btn { background:#2471a3; margin-top:0; }
  .delete-btn { background:#922b21; margin-top:0; }
  .leave-game-btn { background:#5a4a35; font-size:0.8rem; padding:0.4rem 0.9rem; }
  .start-btn { background:#1e8449; font-size:1.2rem; padding:0.75rem 2rem; }

  .player-list { display:flex; gap:0.5rem; flex-wrap:wrap; margin:0.5rem 0; }
  .player-chip {
    display:flex; align-items:center; gap:0.3rem; padding:0.3rem 0.7rem;
    min-width:0; max-width:100%;
    border:2px solid; border-radius:20px; font-size:0.9rem;
    background: var(--chrome-bg-2);
  }
  .chip-dot { display:inline-block; flex:none; width:10px; height:10px; border-radius:50%; }
  .chip-name { min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .retry-btn { margin:0 0 0 0.6rem; padding:0.3rem 0.9rem; font-size:0.9rem; }
  .hint { color:var(--chrome-faint); font-size:0.85rem; }

  /* Game over — foglio segnapunti su pergamena */
  .game-over { padding:2rem; text-align:center; overflow-y:auto; height:100%; box-sizing:border-box; }
  .game-over table {
    margin:1.5rem auto; border-collapse:collapse;
    background: linear-gradient(160deg, var(--board-1), var(--board-2));
    color: var(--board-ink);
    border: 6px solid #6d5228;
    border-radius: 12px;
    box-shadow: var(--shadow-board);
    overflow: hidden;
  }
  th, td { padding:0.55rem 1.1rem; border-bottom:1px solid rgba(109,87,49,0.4); text-align:right; }
  th { color:var(--gold-deep); font-size:0.85rem; text-transform:uppercase; letter-spacing:0.05em; }
  td:first-child, th:first-child { text-align:left; }
  .total { font-weight:800; font-size:1.2rem; color:var(--gold-deep); }
  .winner td { background:rgba(30,132,73,0.14); }
  .longest-path { margin-top:1rem; font-size:1.2rem; color:var(--gold); }
  .winner-line { font-size:2rem; font-weight:800; color:var(--gold); margin-bottom:1rem; }
  /* Mappa a fine partita, sotto il segnapunti: altezza esplicita (il contenitore della mappa riempie). */
  .gameover-map { display:flex; height:60vh; min-height:320px; max-width:1400px; margin:1.5rem auto 0; text-align:left; }

  /* Game layout — plancia segnapunti + board */
  .game-layout { display:flex; width:100vw; height:100vh; }
  .sidebar {
    width:280px; min-width:280px;
    background: var(--chrome-bg-1);
    background-image: var(--table-felt);
    padding:0.75rem; overflow-y:auto;
    border-right:1px solid var(--chrome-line);
    /* Colonna: "Ultime mosse" cede altezza per prima, così le carte scoperte restano in vista. */
    display:flex; flex-direction:column; box-sizing:border-box;
  }
  .sidebar > :global(*) { flex:none; }
  .sidebar > :global(.recent-moves) { flex:0 1 auto; min-height:4.5rem; }
  .player-card {
    background: var(--chrome-bg-2);
    padding:0.55rem 0.65rem; margin-bottom:0.55rem;
    border-radius:10px;
    border:1px solid var(--chrome-line);
    box-shadow: var(--shadow-panel);
  }
  .player-card.current-turn {
    border-color: var(--gold);
    box-shadow: 0 0 0 1px var(--gold), var(--shadow-panel);
  }
  .player-card.offline { opacity:0.72; }
  .offline-tag {
    font-size:0.6rem; font-weight:800; text-transform:uppercase;
    letter-spacing:0.06em; color:#3a1207; background:var(--danger);
    padding:0.1rem 0.4rem; border-radius:999px;
  }
  /* QR di rientro (presence) */
  .offline-strip {
    display:flex; align-items:center; gap:0.6rem;
    background: var(--chrome-bg-2);
    border:1px solid var(--danger);
    border-radius:10px; padding:0.5rem 0.6rem; margin:0.25rem 0 0.75rem;
    box-shadow: var(--shadow-panel);
  }
  .offline-strip-text { font-size:0.8rem; line-height:1.3; }
  .offline-sub { display:block; font-size:0.68rem; color:var(--chrome-muted); margin-top:0.15rem; }
  .offline-qr {
    width:96px; height:96px; margin-left:auto;
    background:#fff; border-radius:8px; padding:4px;
  }
  .player-name { font-weight:bold; display:flex; align-items:center; gap:0.4rem; min-width:0; }
  .pawn { display:inline-flex; width:34px; height:16px; }
  .turn-tag {
    margin-left:auto; font-size:0.6rem; font-weight:800; text-transform:uppercase;
    letter-spacing:0.06em; color:var(--chrome-bg-0); background:var(--gold);
    padding:0.1rem 0.4rem; border-radius:999px;
  }
  .score-line { display:flex; align-items:baseline; gap:0.35rem; margin-top:0.15rem; }
  .score-num { font-size:1.5rem; font-weight:800; color:var(--gold); line-height:1; }
  .score-label { font-size:0.65rem; text-transform:uppercase; letter-spacing:0.08em; color:var(--chrome-muted); }
  .player-stats { display:flex; justify-content:space-between; font-size:0.85rem; margin-top:0.3rem; color:var(--chrome-muted); }
  .player-extras { display:flex; flex-wrap:wrap; gap:0.4rem; font-size:0.8rem; margin-top:0.25rem; color:var(--chrome-muted); }
  .passenger-chip { display:inline-flex; align-items:center; gap:0.2rem; }
  .passenger-dot { display:inline-block; width:0.8rem; height:0.8rem; border-radius:50%; border:1px solid rgba(0,0,0,0.5); }
  .country-stacks { margin-top:0.5rem; font-size:0.85rem; }
  .country-stack { display:flex; justify-content:space-between; gap:0.5rem; color:var(--chrome-muted); }
  .visible-deck { margin-top:1rem; }
  .sidebar-head { display:flex; align-items:center; justify-content:space-between; gap:0.5rem; margin-bottom:0.5rem; }
  .sidebar-head h2 { margin:0; }
  .cards-row { display:flex; gap:6px; flex-wrap:wrap; }
  .face-up-cell { flex:0 0 auto; }
  /* Margine per il badge conteggio del mazzo (sporge in basso a destra) */
  .deck-cell { margin:2px 14px 12px 6px; }
  .mini-card.empty {
    width:44px; aspect-ratio:2/3; border-radius:var(--card-radius);
    display:flex; align-items:center; justify-content:center;
    background:var(--chrome-bg-2); color:var(--chrome-faint);
    border:1px dashed var(--chrome-line); font-weight:bold;
  }
  .last-round-banner {
    background:#c0392b; color:#fff; text-align:center; padding:0.5rem;
    border-radius:8px; margin-top:0.75rem; font-weight:bold;
    border:1px solid rgba(0,0,0,0.4);
    box-shadow: 0 3px 0 rgba(0,0,0,0.35);
    animation:pulse 1s infinite alternate;
  }
  @keyframes pulse { from { opacity:0.8; } to { opacity:1; } }

  /* Banner di stato (errori e disconnessione) */
  .tv-banners {
    position:fixed; top:0.5rem; left:50%; transform:translateX(-50%);
    display:flex; flex-direction:column; gap:0.4rem; align-items:center;
    z-index:50; pointer-events:none;
  }
  .tv-banner {
    pointer-events:auto;
    padding:0.5rem 1.2rem; border-radius:8px; font-size:1rem; font-weight:600;
    background: var(--danger); color:#fff; border:none;
    box-shadow: var(--shadow-panel);
  }

  /* Resume: riassegnazione dei posti salvati */
  .missing-players { margin-top:1rem; }
  .missing-row { display:flex; align-items:center; gap:0.8rem; margin:0.4rem 0; }
  .missing-qr { width:120px; height:120px; border-radius:6px; background:#fff; }
  .rejoin-action { display:flex; flex-wrap:wrap; align-items:center; gap:0.6rem; margin-top:0.5rem; }
  .missing-hint { color:var(--chrome-muted); font-size:0.85rem; }

  /* Tunnel (Europe): carte rivelate in primo piano sopra la mappa */
  .tunnel-banner {
    position:fixed; left:calc(280px + (100vw - 280px) / 2); bottom:1.5rem; transform:translateX(-50%);
    z-index:40; min-width:320px; max-width:calc(100vw - 340px);
    background: var(--chrome-bg-1); border:2px solid var(--gold); border-radius:14px;
    padding:0.8rem 1.4rem 1rem; text-align:center; box-shadow: var(--shadow-board);
  }
  .tunnel-banner h2 { margin:0 0 0.2rem; font-size:1.5rem; }
  .tunnel-who { margin:0 0 0.4rem; color:var(--chrome-muted); font-size:1.1rem; }
  .borrowed-routes { margin-top:1rem; font-size:1.05rem; color:var(--chrome-muted); }
  .borrowed-row { margin:0.3rem 0; }
</style>