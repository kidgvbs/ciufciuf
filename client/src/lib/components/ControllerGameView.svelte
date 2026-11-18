<script lang="ts">
  // Vista di gioco per il controller (telefono).
  // Mostra: mano, biglietti, azioni legali, scelta biglietti.

  import { tick } from 'svelte';
  import { publicStore, privateStore, connectionStore } from '../stores.svelte.ts';
  import { PLAYER_COLOR_CSS, CARD_COLOR_NAME } from '../colors';
  import { errorMessage } from '../errors';
  import { nextClientSeq } from '../client-seq';
  import { createDrawLatch } from '../ticket-draw';
  import { offerKey, isDialogOpen, nextDismissedKey } from '../ticket-offer';
  import { payableRouteIds } from '../payment';
  import { rulesetHasStations } from '../scores';
  import {
    addedColors,
    countByColor,
    flyCard,
    isFreshDrawSource,
    rectOf,
    type DrawSource,
  } from '../draw-animation';
  import type { InboundAction, CardColor, PassengerPick, TunnelReveal } from '../proto';
  import { passengerChoices } from '../passengers';
  import TicketChoiceDialog from './TicketChoiceDialog.svelte';
  import SpecialRulesDialog from './SpecialRulesDialog.svelte';
  import MapSvg from './MapSvg.svelte';
  import TrainCard from './TrainCard.svelte';
  import PaymentPanel from './PaymentPanel.svelte';
  import TunnelCards from './TunnelCards.svelte';
  import ClaimDialog from './ClaimDialog.svelte';
  import TicketColumn from './TicketColumn.svelte';
  import DrawArea from './DrawArea.svelte';
  import { gameMap } from '../game-map.svelte.ts';
  import { rulesetCatalog } from '../rulesets.svelte.ts';
  import { autoOpensRules, hasSeenRules, markRulesSeen } from '../special-rules';

  const mapData = $derived(gameMap.state.data);
  const ticketMap = $derived(new Map((mapData?.tickets ?? []).map((t) => [t.id, t])));
  const routeMap = $derived(new Map((mapData?.routes ?? []).map((r) => [r.id, r])));
  const cityMap = $derived(new Map((mapData?.cities ?? []).map((c) => [c.id, c])));

  function cityName(id: string): string {
    return cityMap.get(id)?.name ?? id;
  }

  /** Scroll solo se serve, senza animazione con `prefers-reduced-motion`. */
  function revealEl(el: HTMLElement | undefined) {
    if (!el) return;
    const reduce = typeof window.matchMedia === 'function'
      && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    el.scrollIntoView({ behavior: reduce ? 'auto' : 'smooth', block: 'nearest' });
  }

  let { playerId, onLeave, sendAction, drawBusy }: {
    playerId: string;
    onLeave: () => void;
    sendAction: (a: InboundAction) => boolean;
    drawBusy: boolean;
  } = $props();

  // ── Derived ────────────────────────────────────────────────────
  const gameState = $derived(publicStore.state);
  const isMyTurn = $derived(publicStore.currentTurnPlayerId === playerId);
  const mySummary = $derived(
    publicStore.playersSummary.find((p) => p.player_id === playerId),
  );
  const hand = $derived(privateStore.hand);
  const tickets = $derived(privateStore.tickets);
  const offered = $derived(privateStore.offeredTickets);
  const legalMoves = $derived(privateStore.legalMoves);

  // Mano raggruppata per colore (funzione pura, chiamata nella template)
  function getHandByColor(): [CardColor, number][] {
    const m = new Map<CardColor, number>();
    for (const c of hand) {
      m.set(c, (m.get(c) ?? 0) + 1);
    }
    return [...m.entries()].sort((a, b) => b[1] - a[1]);
  }

  // Stato locale
  // Colore selezionato nella mano (tocco su una carta, secondo tocco deseleziona):
  // la mappa evidenzia solo le rotte pagabili con quel colore più i jolly.
  let selectedColor = $state<CardColor | null>(null);
  // Mappa sul telefono: visibile di default, collassabile
  let showMap = $state(true);

  // Biglietti pinnati sulla mappa: MULTI-pin — ogni
  // biglietto cliccato aggiunge/rimuove il proprio tratteggiato, senza
  // spegnere gli altri. Il tratteggiato sparisce anche quando il SERVER
  // dichiara il biglietto completato (filtro su t.completed: zero
  // pathfinding nel client). Stato locale del controller (non persistito);
  // la TV non riceve mai questi dati (biglietti privati).
  let pinnedTicketIds = $state<string[]>([]);
  const completedTicketIds = $derived(
    new Set(tickets.filter((t) => t.completed).map((t) => t.id))
  );
  const highlightTickets = $derived.by(() => {
    const out: Array<{ from: string; to: string }> = [];
    for (const id of pinnedTicketIds) {
      if (completedTicketIds.has(id)) continue;
      const t = ticketMap.get(id);
      if (t) out.push({ from: t.from, to: t.to });
    }
    return out;
  });

  function toggleTicketHighlight(ticketId: string) {
    pinnedTicketIds = pinnedTicketIds.includes(ticketId)
      ? pinnedTicketIds.filter((id) => id !== ticketId)
      : [...pinnedTicketIds, ticketId];
  }

  // Rotta in pagamento: aperta nel popup ClaimDialog (tap su una rotta evidenziata)
  let payingRouteId = $state<string | null>(null);

  // Stazione (Europe): modalità "costruisci", città scelta (mappa o lista)
  let buildingStation = $state(false);
  let stationCity = $state<string | null>(null);
  let stationTab = $state<'map' | 'list'>('map');
  let mapEl = $state<HTMLDivElement>();
  let stationPayEl = $state<HTMLDivElement>();

  // Regole speciali del ruleset (testi dal catalogo del server). A inizio partita il modale viene
  // PRIMA del dialog dei biglietti; «Ho capito» lo chiude e lo ricorda per questa partita. Il 📜
  // lo riapre quando si vuole.
  const gameId = $derived(publicStore.gameId);
  const specialRules = $derived(rulesetCatalog.specialRules(gameMap.rulesetId));
  const rulesTitle = $derived(rulesetCatalog.title(gameMap.rulesetId));
  let rulesSeen = $state(hasSeenRules(publicStore.gameId));
  let rulesManual = $state(false);
  $effect(() => {
    rulesSeen = hasSeenRules(gameId);
  });
  const rulesOpen = $derived(rulesManual || autoOpensRules(specialRules.length, gameState, rulesSeen));
  // Durante il draft, finché il ruleset non è noto non si sa se ci sono regole da mostrare: il
  // dialog dei biglietti aspetta (l'offerta resta) invece di aprirsi e venire coperto subito dopo.
  const rulesPending = $derived(
    gameState === 'INITIAL_DRAFT' &&
      !rulesSeen &&
      (gameMap.rulesetId === ''
        ? gameMap.state.status !== 'error'
        : rulesetCatalog.status === 'idle' || rulesetCatalog.status === 'loading'),
  );

  function closeRules() {
    rulesManual = false;
    rulesSeen = true;
    markRulesSeen(gameId);
  }

  // Flash "biglietto completato" (§8.2): salta il primo sync (snapshot/reconnect)
  let knownCompleted: Set<string> | null = null;
  let ticketFlash = $state<string[]>([]);
  let flashTimer: ReturnType<typeof setTimeout> | null = null;

  // Il dialog dei biglietti si apre alla comparsa di un'offerta nuova; "Annulla" lo tiene chiuso
  // finché l'offerta non cambia. Quando l'offerta sparisce la chiusura manuale viene dimenticata.
  const offerId = $derived(offerKey(offered));
  let dismissedOfferId = $state('');
  const ticketDialogWanted = $derived(isDialogOpen(offerId, dismissedOfferId));
  const ticketDialogOpen = $derived(ticketDialogWanted && !rulesOpen && !rulesPending);
  const ticketChoice = $derived(legalMoves?.ticket_choice ?? null);
  $effect(() => {
    dismissedOfferId = nextDismissedKey(offerId, dismissedOfferId);
  });

  // Flash verde quando un biglietto passa a completato
  $effect(() => {
    const completedNow = new Set(tickets.filter((t) => t.completed).map((t) => t.id));
    if (knownCompleted === null) {
      knownCompleted = completedNow;
      return;
    }
    const fresh = [...completedNow].filter((id) => !knownCompleted!.has(id));
    knownCompleted = completedNow;
    if (fresh.length > 0) {
      ticketFlash = fresh;
      if (flashTimer) clearTimeout(flashTimer);
      flashTimer = setTimeout(() => { ticketFlash = []; }, 3500);
    }
  });

  // Il banner di errore si auto-chiude dopo pochi secondi. Il cleanup
  // cancella il timer quando arriva un nuovo errore o allo smontaggio.
  $effect(() => {
    if (!connectionStore.lastError) return;
    const t = setTimeout(() => { connectionStore.lastError = null; }, 4000);
    return () => clearTimeout(t);
  });

  // Cancella i timer one-shot allo smontaggio del componente
  $effect(() => () => {
    if (flashTimer) clearTimeout(flashTimer);
  });

  // Vibrazione quando diventa il mio turno (silenziosa dove non supportata)
  let wasMyTurn = false;
  $effect(() => {
    const now = isMyTurn && canAct;
    if (now && !wasMyTurn) {
      navigator.vibrate?.(200);
    }
    wasMyTurn = now;
  });

  // ── Azioni ─────────────────────────────────────────────────────
  function drawDeck(source: HTMLElement) {
    const rect = rectOf(source);
    const sent = sendAction({
      actionType: 'DRAW_TRAIN_CARD',
      player_id: playerId,
      source: 'deck',
      client_seq: nextClientSeq(playerId),
    });
    if (sent) drawSource = { rect, at: Date.now() };
  }

  function drawFaceUp(index: number, source: HTMLElement) {
    const rect = rectOf(source);
    const sent = sendAction({
      actionType: 'DRAW_TRAIN_CARD',
      player_id: playerId,
      source: { faceUp: { index } },
      client_seq: nextClientSeq(playerId),
    });
    if (sent) drawSource = { rect, at: Date.now() };
  }

  // ── Animazione pesca (§8.2) ────────────────────────────────────
  // Il diff della mano (dal server) dice quale carta è entrata; la posizione di partenza è quella
  // del bottone toccato. Nessuna regola di gioco: se la mano cresce senza una pesca appena
  // inviata da qui (snapshot, reconnect) non si anima nulla.
  let drawSource: DrawSource | null = null;
  let handFanEl = $state<HTMLDivElement>();
  let rootEl = $state<HTMLDivElement>();
  let prevHandCounts: Map<CardColor, number> | null = null;
  $effect(() => {
    const counts = countByColor(hand);
    const prev = prevHandCounts;
    prevHandCounts = counts;
    if (prev === null) return;
    const added = addedColors(prev, counts);
    const source = drawSource;
    if (added.length === 0 || !source) return;
    drawSource = null;
    if (!isFreshDrawSource(source, Date.now())) return;
    void tick().then(() => {
      for (const color of added) {
        const card = handFanEl?.querySelector<HTMLElement>(`[data-card-color="${color}"]`);
        if (card && rootEl) flyCard(source.rect, card, rootEl);
      }
    });
  });

  // Pesca rifiutata dal server (errore dopo l'invio): niente animazione per una carta che non arriva.
  $effect(() => {
    if (connectionStore.lastError) drawSource = null;
  });

  // Pesca biglietti: la carta nella colonna dei biglietti manda DRAW_TICKETS (quanti se ne
  // pescano e quanti se ne tengono lo decide il server).
  // Anti doppio tap: la carta resta attiva finché non arriva l'offerta.
  let drawingTickets = $state(false);
  const drawLatch = createDrawLatch((pending) => (drawingTickets = pending));
  $effect(() => {
    // Tutte le dipendenze vanno lette prima: niente corto circuito che ne nasconda una.
    const hasOffer = offered.length > 0;
    const error = connectionStore.lastError;
    const canDraw = (legalMoves?.can_draw_tickets ?? false) && canAct;
    if (hasOffer || !canDraw) drawLatch.release();
    else drawLatch.releaseOnError(error);
  });

  function drawTickets() {
    drawLatch.submit(
      () =>
        sendAction({
          actionType: 'DRAW_TICKETS',
          player_id: playerId,
          client_seq: nextClientSeq(playerId),
        }),
      connectionStore.lastError,
    );
  }

  // ── Pagamento rotta ────────────────────────────────────────────
  // Tap su una rotta evidenziata → popup (ClaimDialog) dove si sceglie colore, locomotive e
  // passeggeri; il server valida il pagamento (errore mostrato come banner).

  const payingRoute = $derived(
    payingRouteId ? (routeMap.get(payingRouteId) ?? null) : null,
  );
  // Passeggeri (Germany 2017): negli estremi con più colori il giocatore sceglie quale prendere.
  const passengerChoiceList = $derived(
    payingRoute ? passengerChoices(publicStore.passengers, payingRoute) : [],
  );

  // Tap-to-claim dalla mappa: solo le rotte evidenziate (legali per il server e pagabili con
  // la carta selezionata) aprono il popup. Client scemo — la verità è legalMoves.
  function onRouteTap(routeId: string) {
    if (!canAct || buildingStation || !claimableRouteIds.includes(routeId)) return;
    payingRouteId = routeId;
  }

  function cancelPayment() {
    payingRouteId = null;
  }

  function confirmClaim(cards: CardColor[], passengers: PassengerPick[] | undefined) {
    const route = payingRoute;
    if (!route) return;
    const sent = sendAction({
      actionType: 'CLAIM_ROUTE',
      player_id: playerId,
      route_id: route.id,
      cards_discarded: cards,
      passengers,
      client_seq: nextClientSeq(playerId),
    });
    // Se l'azione non è partita il popup resta aperto per riprovare.
    if (!sent) return;

    payingRouteId = null;
    selectedColor = null;
  }

  // ── Stazioni (Europe) ──────────────────────────────────────────
  // Le città ammesse e il costo arrivano da `legalMoves`; qui si sceglie solo la città
  // (tap sulla mappa o lista) e le carte (colore + locomotive).
  const stationCities = $derived(legalMoves?.station_cities ?? []);
  const stationCost = $derived(legalMoves?.station_cost ?? 0);
  // Lista testuale: per nome, solo le città ammesse dal server.
  const stationCityList = $derived(
    [...stationCities].sort((a, b) => cityName(a).localeCompare(cityName(b))),
  );
  const hasStations = $derived(
    rulesetHasStations(publicStore.playersSummary, publicStore.stations),
  );
  // Scelta le carte per la stazione: popup come per il tunnel (niente scroll nella colonna).
  const stationPaying = $derived(buildingStation && stationCity !== null);

  // La modalità si chiude da sola se non è più possibile costruire (turno passato, carte cambiate).
  $effect(() => {
    if (!canAct || stationCities.length === 0) {
      buildingStation = false;
      stationCity = null;
    } else if (stationCity !== null && !stationCities.includes(stationCity)) {
      stationCity = null;
    }
  });

  // Scelta della città sulla mappa: la mappa (sopra in portrait) torna in vista.
  $effect(() => {
    if (buildingStation && stationCity === null && stationTab === 'map') revealEl(mapEl);
  });

  // Da tastiera, scelta la città il focus raggiunge il popup di pagamento (lo sfondo è inerte,
  // ma il focus resterebbe sul pulsante della città). Con il tocco non serve.
  let lastInputWasKeyboard = false;
  $effect(() => {
    const onKey = () => { lastInputWasKeyboard = true; };
    const onPointer = () => { lastInputWasKeyboard = false; };
    document.addEventListener('keydown', onKey, true);
    document.addEventListener('pointerdown', onPointer, true);
    return () => {
      document.removeEventListener('keydown', onKey, true);
      document.removeEventListener('pointerdown', onPointer, true);
    };
  });
  $effect(() => {
    if (!buildingStation || stationCity === null || !lastInputWasKeyboard) return;
    void tick().then(() => {
      stationPayEl
        ?.querySelector<HTMLElement>('button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])')
        ?.focus();
    });
  });

  function startStation() {
    buildingStation = true;
    stationCity = null;
    payingRouteId = null;
    showMap = true;
  }

  function cancelStation() {
    buildingStation = false;
    stationCity = null;
  }

  function onCityTap(cityId: string) {
    if (buildingStation && stationCities.includes(cityId)) stationCity = cityId;
  }

  function confirmStation(cards: CardColor[]) {
    if (!stationCity) return;
    const sent = sendAction({
      actionType: 'BUILD_STATION',
      player_id: playerId,
      city_id: stationCity,
      cards_discarded: cards,
      client_seq: nextClientSeq(playerId),
    });
    // Se l'azione non è partita il pannello resta aperto per riprovare.
    if (!sent) return;
    cancelStation();
  }

  // ── Tunnel (Europe) ────────────────────────────────────────────
  // Decisione richiesta dal server (`legalMoves.tunnel`): le carte rivelate sono pubbliche,
  // costo extra e colore arrivano dalla mossa legale.
  const tunnelMove = $derived(legalMoves?.tunnel ?? null);
  const tunnelRoute = $derived(tunnelMove ? (routeMap.get(tunnelMove.route_id) ?? null) : null);
  const tunnelView = $derived.by((): TunnelReveal | null => {
    if (!tunnelMove) return null;
    const pub = publicStore.tunnelReveal;
    return {
      route_id: tunnelMove.route_id,
      cards: pub && pub.route_id === tunnelMove.route_id ? pub.cards : [],
      color: tunnelMove.color,
      extra_cost: tunnelMove.extra_cost,
    };
  });

  // Il dialog prende il focus all'apertura (lo sfondo è inerte).
  let tunnelDialogEl = $state<HTMLDivElement>();
  $effect(() => {
    tunnelDialogEl?.focus();
  });

  function decideTunnel(pay: boolean, cards: CardColor[]) {
    sendAction({
      actionType: 'TUNNEL_DECISION',
      player_id: playerId,
      pay,
      cards_discarded: pay ? cards : [],
      client_seq: nextClientSeq(playerId),
    });
  }

  function confirmTicketChoice(keep: string[]) {
    const sent = sendAction({
      actionType: 'CHOOSE_TICKETS',
      player_id: playerId,
      keep,
      client_seq: nextClientSeq(playerId),
    });
    if (sent) dismissedOfferId = offerId;
  }

  function cancelTicketChoice() {
    dismissedOfferId = offerId;
  }

  // ── Label stato ────────────────────────────────────────────────
  // ($derived.by: il valore È il testo, non una funzione che lo calcola)
  const statusLabel = $derived.by(() => {
    if (gameState === 'WAITING_FOR_PLAYERS') return 'In attesa di giocatori...';
    if (gameState === 'INITIAL_DRAFT') {
      return offered.length > 0 ? 'Scegli i biglietti iniziali' : 'Attendi che tutti scelgano...';
    }
    if (gameState === 'PLAYER_TURN' || gameState === 'LAST_ROUND') {
      if (isMyTurn) return gameState === 'LAST_ROUND' ? '⚠️ Ultimo giro — tocca a te!' : 'È il tuo turno!';
      const p = publicStore.playersSummary.find(
        (s) => s.player_id === publicStore.currentTurnPlayerId,
      );
      return gameState === 'LAST_ROUND'
        ? '⚠️ Ultimo giro — turno di ' + (p?.name ?? '...')
        : 'Turno di ' + (p?.name ?? '...');
    }
    if (gameState === 'SECOND_CARD_DRAW') {
      return isMyTurn ? 'Pesca la seconda carta' : 'Attendi...';
    }
    if (gameState === 'TICKET_CHOICE') {
      return isMyTurn && offered.length > 0
        ? 'Scegli i biglietti'
        : 'Attendi...';
    }
    if (gameState === 'TUNNEL_RESOLUTION') {
      return isMyTurn ? 'Tunnel: decidi se pagare' : 'Tunnel in corso...';
    }
    if (gameState === 'STATION_SCORING') return 'Calcolo stazioni...';
    if (gameState === 'GAME_OVER') return '🏁 Partita finita!';
    return gameState;
  });

  const canAct = $derived(
    isMyTurn &&
      (gameState === 'PLAYER_TURN' ||
        gameState === 'LAST_ROUND' ||
        gameState === 'SECOND_CARD_DRAW'),
  );

  // Rotte da evidenziare sulla mappa: le legali (dal server) pagabili con la carta selezionata
  // nella mano, o tutte senza selezione. `payableRouteIds` è solo una guardia di UX.
  const claimableRouteIds = $derived(
    canAct && legalMoves && !buildingStation
      ? payableRouteIds(legalMoves.routes, routeMap, hand, selectedColor)
      : [],
  );

  // Carta selezionata non più in mano (spesa nel pagamento): la selezione cade.
  $effect(() => {
    if (selectedColor !== null && !hand.includes(selectedColor)) selectedColor = null;
  });

  // Il popup si chiude se la rotta non è più reclamabile (turno passato, mosse cambiate).
  $effect(() => {
    if (payingRouteId !== null && (!canAct || !legalMoves?.routes.includes(payingRouteId))) {
      payingRouteId = null;
    }
  });
</script>

<div class="cv-game" bind:this={rootEl}>
  <!-- Sfondo inerte mentre il dialog dei biglietti è aperto. -->
  <div
    class="cv-bg"
    inert={ticketDialogOpen || rulesOpen || tunnelMove !== null || payingRoute !== null || stationPaying}
  >
  <!-- Header -->
  <div class="header">
    <div class="player-info">
      <span
        class="chip-dot"
        style="background:{mySummary
          ? PLAYER_COLOR_CSS[mySummary.color]
          : '#666'}"
      ></span>
      <span class="player-name">{mySummary?.name ?? playerId}</span>
    </div>
    <div class="player-stats">
      <span>⭐ {mySummary?.score ?? 0}</span>
      <span>🚂 {mySummary?.trains_remaining ?? 0}</span>
      {#if hasStations}
        <span title="Stazioni rimaste">🏠 {mySummary?.stations_remaining ?? 0}</span>
      {/if}
      <span title="Carte nel mazzo">🃏 {publicStore.deckSize}</span>
    </div>
    {#if specialRules.length > 0}
      <button
        class="btn rules-btn"
        onclick={() => (rulesManual = true)}
        aria-label="Regole speciali"
        title="Regole speciali"
      >📜</button>
    {/if}
    <button class="btn leave-btn" onclick={onLeave}> Esci </button>
  </div>

  <!-- Status -->
  <div class="status-bar" class:my-turn={isMyTurn && canAct} role="status" aria-live="polite">
    {statusLabel}
  </div>

  {#if publicStore.lastRound && gameState !== 'GAME_OVER'}
    <div class="last-round-banner">⚠️ ULTIMO GIRO</div>
  {/if}

  {#if offerId !== '' && !ticketDialogWanted}
    <button class="btn reopen-tickets" onclick={() => (dismissedOfferId = '')}>
      🎫 Devi scegliere i biglietti ({offered.length})
    </button>
  {/if}

  <!-- Toggle mappa: solo portrait (in landscape la plancia è sempre visibile) -->
  <button
    class="map-toggle"
    onclick={() => (showMap = !showMap)}
    aria-expanded={showMap}
  >
    🗺️ Mappa {showMap ? '▾' : '▸'}
  </button>

  <!-- Layout a 3 colonne (landscape): biglietti | mappa con la mano | pesca carte + azioni.
       Portrait: impilato (mappa, pesca carte, biglietti) via CSS. -->
  <div class="cv-main">
    <div class="cv-left">
      <TicketColumn
        {tickets}
        pinnedIds={pinnedTicketIds}
        onToggle={toggleTicketHighlight}
        canDraw={canAct && (legalMoves?.can_draw_tickets ?? false)}
        busy={drawBusy || drawingTickets}
        onDraw={drawTickets}
        remaining={publicStore.ticketDeckSize}
      />
    </div>

    <!-- Mappa con tap-to-claim: evidenzia le rotte legali (pagabili con la carta
         selezionata in mano, se c'è); il tap su una evidenziata apre il popup di reclamo.
         highlightTickets: i biglietti pinnati. -->
    <div class="cv-map" class:map-hidden={!showMap} bind:this={mapEl}>
      {#if buildingStation && stationCity === null && stationTab === 'map'}
        <div class="map-hint" role="status">🏠 Tocca una città evidenziata per la stazione</div>
      {/if}
      <MapSvg
        compact
        highlightRouteIds={claimableRouteIds}
        selectedRouteId={payingRouteId}
        onRouteTap={onRouteTap}
        highlightTickets={highlightTickets}
        selectableCityIds={buildingStation ? stationCities : []}
        selectedCityId={stationCity}
        onCityTap={onCityTap}
      />
    </div>

    <div class="cv-right">
      <DrawArea
        visibleDeck={publicStore.visibleDeck}
        deckSize={publicStore.deckSize}
        faceUpMoves={canAct && legalMoves ? legalMoves.draw_face_up : []}
        canDrawDeck={canAct && (legalMoves?.can_draw_deck ?? false)}
        busy={drawBusy}
        onDrawDeck={drawDeck}
        onDrawFaceUp={drawFaceUp}
      />

  <!-- Actions -->
  {#if canAct && legalMoves && (legalMoves.routes.length > 0 || stationCities.length > 0)}
    <div class="section actions">
      <h3>Azioni</h3>
      {#if legalMoves.routes.length > 0 && stationCities.length === 0}
        <p class="actions-hint">Tocca una carta, poi una rotta evidenziata.</p>
      {/if}
      {#if stationCities.length > 0}
        {#if !buildingStation}
          <button
            onclick={startStation}
            class="btn action-btn build-station"
            aria-label="Costruisci stazione ({stationCost} {stationCost === 1 ? 'carta' : 'carte'})"
          >
            🏠 Stazione ({stationCost} {stationCost === 1 ? 'carta' : 'carte'})
          </button>
        {:else if !stationCity}
          <div class="route-choices">
            <div class="claim-tabs">
              <button class="btn claim-tab" class:active={stationTab === 'map'} onclick={() => (stationTab = 'map')}>
                🗺️ Mappa
              </button>
              <button class="btn claim-tab" class:active={stationTab === 'list'} onclick={() => (stationTab = 'list')}>
                ☰ Lista
              </button>
            </div>
            <h4>Scegli la città della stazione</h4>
            {#if stationTab === 'list'}
              <div class="city-list">
                {#each stationCityList as cid (cid)}
                  <button onclick={() => (stationCity = cid)} class="btn route-option">
                    <div class="route-name">{cityName(cid)}</div>
                  </button>
                {/each}
              </div>
            {:else}
              <p class="pay-label">Tocca una città evidenziata sulla mappa.</p>
            {/if}
            <button onclick={cancelStation} class="btn cancel-route">Annulla</button>
          </div>
        {/if}
      {/if}
    </div>
  {:else if !canAct && gameState !== 'WAITING_FOR_PLAYERS' && gameState !== 'INITIAL_DRAFT' && gameState !== 'GAME_OVER'}
    <div class="section waiting-section">
      <p class="waiting-text">In attesa...</p>
    </div>
  {/if}
    </div>
  </div>

  <!-- Hand: ventaglio docked in basso, come le carte tenute in mano sul tavolo -->
  <div class="section hand-dock">
    <h3>Mano ({hand.length} carte)</h3>
    {#if hand.length > 0}
      {@const hbc = getHandByColor()}
      {#if hbc.length > 0}
        <div class="hand-fan" bind:this={handFanEl}>
          {#each hbc as [color, count], i}
            {@const rot = (i - (hbc.length - 1) / 2) * 6}
            <button
              type="button"
              class="fan-card"
              data-card-color={color}
              class:focused={selectedColor === color}
              style="--rot:{rot}deg"
              aria-label="{CARD_COLOR_NAME[color]}: {count} in mano"
              aria-pressed={selectedColor === color}
              onclick={() => (selectedColor = selectedColor === color ? null : color)}
            >
              <TrainCard color={color as CardColor} {count} handSize={hand.length} />
            </button>
          {/each}
        </div>
      {:else}
        <p class="empty">Nessuna carta in mano</p>
      {/if}
    {/if}
  </div>

  <!-- Connection status -->
  {#if !connectionStore.connected}
    <div class="disconnected-banner">
      ⚠️ Disconnesso &mdash; riconnessione in corso...
    </div>
  {/if}

  {#if connectionStore.lastError}
    <div role="alert">
      <button class="error-banner" onclick={() => (connectionStore.lastError = null)}>
        ❌ {errorMessage(connectionStore.lastError)}
      </button>
    </div>
  {/if}

  {#if ticketFlash.length > 0}
    <div class="ticket-flash">
      ✅ Bigliett{ticketFlash.length > 1 ? 'i completati' : 'o completato'}:
      {ticketFlash
        .map((id) => {
          const d = ticketMap.get(id);
          return d ? cityName(d.from) + ' → ' + cityName(d.to) : id;
        })
        .join(', ')}
    </div>
  {/if}

  </div>

  <!-- Regole speciali: prima della scelta dei biglietti a inizio partita, o dal pulsante 📜 -->
  {#if rulesOpen}
    <SpecialRulesDialog title={rulesTitle} rules={specialRules} onClose={closeRules} />
  {/if}
  <!-- Ticket choice dialog -->
  {#if ticketDialogOpen}
    <TicketChoiceDialog
      offeredTickets={offered}
      choice={ticketChoice}
      onConfirm={confirmTicketChoice}
      onCancel={cancelTicketChoice}
    />
  {/if}
  <!-- Stazione (Europe): pagamento in un popup, come il tunnel -->
  {#if stationPaying && stationCity}
    <div class="tunnel-overlay">
      <div
        class="tunnel-dialog wide"
        role="dialog"
        aria-modal="true"
        aria-label="Costruisci stazione"
        tabindex="-1"
        bind:this={stationPayEl}
        onkeydown={(e: KeyboardEvent) => { if (e.key === 'Escape') cancelStation(); }}
      >
        <PaymentPanel
          total={stationCost}
          {hand}
          title="Stazione a {cityName(stationCity)} ({stationCost} {stationCost === 1 ? 'carta' : 'carte'})"
          confirmLabel="✅ Costruisci stazione"
          busy={drawBusy}
          onConfirm={confirmStation}
          backLabel="Cambia città"
          onBack={() => (stationCity = null)}
        />
        <button onclick={cancelStation} class="btn cancel-route">Annulla</button>
      </div>
    </div>
  {/if}
  <!-- Reclamo rotta: popup con colore, locomotive, passeggeri; ✅ invia, ❌ annulla -->
  {#if payingRoute}
    <ClaimDialog
      route={payingRoute}
      {cityName}
      {hand}
      {selectedColor}
      {passengerChoiceList}
      onConfirm={confirmClaim}
      onCancel={cancelPayment}
    />
  {/if}
  <!-- Tunnel (Europe): decisione bloccante finché il server la richiede -->
  {#if tunnelMove && tunnelView}
    <div class="tunnel-overlay">
      <div class="tunnel-dialog" role="dialog" aria-modal="true" aria-labelledby="tunnel-title"
        tabindex="-1" bind:this={tunnelDialogEl}>
        <h2 id="tunnel-title">
          🚇 Tunnel{tunnelRoute ? `: ${cityName(tunnelRoute.from)} → ${cityName(tunnelRoute.to)}` : ''}
        </h2>
        <TunnelCards reveal={tunnelView} />
        {#if tunnelMove.can_pay}
          <PaymentPanel
            total={tunnelMove.extra_cost}
            {hand}
            fixedColor={tunnelMove.color}
            title="Carte extra da pagare"
            confirmLabel="Paga {tunnelMove.extra_cost}"
            busy={drawBusy}
            onConfirm={(cards) => decideTunnel(true, cards)}
            backLabel="Rinuncia"
            onBack={() => decideTunnel(false, [])}
          />
        {:else}
          <p class="tunnel-nopay">Non hai carte sufficienti per pagare il costo extra.</p>
          <button class="btn cancel-route tunnel-giveup" onclick={() => decideTunnel(false, [])}>
            Rinuncia
          </button>
        {/if}
        {#if connectionStore.lastError}
          <p class="pay-invalid" role="alert">{errorMessage(connectionStore.lastError)}</p>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  /* ── Tavolo (telefono): chrome scuro caldo, carte chiare ────────── */
  .cv-game {
    min-height: 100vh;
    min-height: 100dvh;
    background: var(--chrome-bg-0);
    background-image: var(--table-felt);
    color: var(--chrome-text);
    font-family: system-ui, sans-serif;
    padding-bottom: 2rem;
  }

  .cv-bg {
    display: contents;
  }

  /* Header */
  .header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding-top: calc(0.75rem + env(safe-area-inset-top));
    padding-left: calc(1rem + env(safe-area-inset-left));
    padding-right: calc(1rem + env(safe-area-inset-right));
    background: var(--chrome-bg-1);
    border-bottom: 1px solid var(--chrome-line);
    position: sticky;
    top: 0;
    z-index: 10;
  }
  .player-info {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    flex: 1;
    min-width: 0;
  }
  .chip-dot {
    display: inline-block;
    width: 12px;
    height: 12px;
    border-radius: 50%;
  }
  .player-name {
    font-weight: 600;
    font-size: 1rem;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .player-stats {
    display: flex;
    gap: 1rem;
    font-size: 0.9rem;
    color: var(--gold);
  }
  .leave-btn {
    background: #5a4a35;
    color: #fff;
    padding: 0.35rem 0.75rem;
    font-size: 0.8rem;
  }
  .rules-btn {
    background: var(--chrome-bg-2);
    border: 1px solid var(--chrome-line);
    padding: 0.25rem 0.5rem;
    font-size: 0.95rem;
    line-height: 1;
  }

  /* Status */
  .status-bar {
    padding: 0.5rem 1rem;
    background: var(--chrome-bg-2);
    text-align: center;
    font-size: 0.9rem;
    color: var(--chrome-muted);
    border-bottom: 1px solid var(--chrome-line);
  }
  .status-bar.my-turn {
    background: var(--gold);
    color: #453416;
    font-weight: 700;
    animation: pulse 1s infinite alternate;
  }
  @keyframes pulse {
    from {
      opacity: 0.85;
    }
    to {
      opacity: 1;
    }
  }

  /* Sections (vassoi sul tavolo) */
  .section {
    margin: 0.75rem 1rem;
    background: var(--chrome-bg-1);
    border: 1px solid var(--chrome-line);
    border-radius: 12px;
    padding: 0.75rem;
    box-shadow: var(--shadow-panel);
  }
  .section h3 {
    margin: 0 0 0.5rem;
    font-size: 0.9rem;
    color: var(--gold);
  }
  .section h4 {
    margin: 0.5rem 0;
    font-size: 0.85rem;
    color: var(--gold);
  }

  /* Mano a ventaglio */
  .hand-fan {
    display: flex;
    align-items: flex-end;
    justify-content: center;
    padding: 0.4rem 0.2rem 0.2rem;
  }
  .fan-card {
    /* reset del <button> */
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    cursor: pointer;
    transform: rotate(var(--rot, 0deg));
    transform-origin: 50% 110%;
    transition: transform 0.15s ease;
  }
  .fan-card + .fan-card {
    margin-left: -22px;
  }
  .fan-card:hover {
    transform: rotate(var(--rot, 0deg)) translateY(-6px);
  }
  /* Carta toccata: sopra le altre, dritta, sollevata e un po' più grande,
     così colore e numero si leggono anche nel ventaglio sovrapposto. */
  .fan-card.focused {
    position: relative;
    z-index: 2;
    transform: rotate(0deg) translateY(-14px) scale(1.25);
  }
  /* Carta selezionata (filtra le rotte sulla mappa): bordo dorato, oltre al sollevamento */
  .fan-card.focused :global(.train-card) {
    border-color: var(--gold);
    box-shadow:
      0 0 0 2px var(--gold),
      var(--card-shadow);
  }

  /* Actions */
  .actions {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .action-btn {
    padding: 0.75rem;
    font-size: 1rem;
    text-align: center;
    width: 100%;
  }
  .actions-hint {
    margin: 0;
    font-size: 0.8rem;
    color: var(--chrome-muted);
  }

  /* ── Layout principale: biglietti | mappa | pesca carte + azioni ─
     Portrait (default): colonna — mappa (collassabile), pesca carte, biglietti. */
  .cv-main {
    display: flex;
    flex-direction: column;
  }
  .cv-map {
    position: relative;
    /* scrollIntoView non deve infilare la mappa sotto l'header sticky. */
    scroll-margin-top: 4.5rem;
    margin: 0.75rem 1rem 0;
    height: 240px;
  }
  .cv-map.map-hidden {
    display: none;
  }
  .map-hint {
    position: absolute;
    top: 0.4rem;
    left: 50%;
    transform: translateX(-50%);
    z-index: 5;
    max-width: calc(100% - 1rem);
    padding: 0.3rem 0.7rem;
    border: 1px solid var(--gold);
    border-radius: 999px;
    background: rgba(35, 26, 17, 0.92);
    color: #fbf6e9;
    font-size: 0.8rem;
    font-weight: 600;
    text-align: center;
    pointer-events: none;
  }
  .cv-left {
    order: 3;
    display: flex;
    flex-direction: column;
    margin: 0.75rem 1rem 0;
  }
  .cv-left > :global(.ticket-col) {
    flex: 1 1 auto;
    min-height: 0;
  }
  .cv-map {
    order: 1;
  }
  .cv-right {
    order: 2;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    margin: 0.75rem 1rem 0;
  }
  .cv-right > :global(*) {
    flex-shrink: 0;
  }
  .cv-right .section {
    margin: 0;
  }
  .map-toggle {
    display: block;
    width: calc(100% - 2rem);
    margin: 0.5rem 1rem 0;
    background: var(--chrome-bg-1);
    border: 1px solid var(--chrome-line);
    border-radius: 10px;
    color: var(--gold);
    font-size: 0.9rem;
    font-weight: 600;
    text-align: left;
    padding: 0.4rem 0.6rem;
    cursor: pointer;
  }

  /* Mano docked in basso (le "carte in mano" sul bordo del tavolo) */
  .hand-dock {
    margin: 0.75rem 0.5rem 0;
    padding: 0.4rem 0.5rem 0.6rem;
    padding-bottom: calc(0.6rem + env(safe-area-inset-bottom));
  }
  .hand-dock h3 {
    margin: 0 0 0.25rem;
    padding: 0 0.4rem;
  }

  /* Tab claim mappa/lista */
  .claim-tabs {
    display: flex;
    gap: 6px;
  }
  .claim-tab {
    flex: 1;
    background: var(--chrome-bg-2);
    color: var(--chrome-muted);
    padding: 0.4rem;
    font-size: 0.85rem;
  }
  .claim-tab.active {
    background: var(--gold);
    color: #453416;
  }

  /* Route choices */
  .route-choices {
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--chrome-bg-0);
    border-radius: 10px;
    padding: 0.75rem;
  }
  .route-option {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.6rem 0.75rem;
    background: var(--chrome-bg-2);
    color: var(--chrome-text);
    text-align: left;
    width: 100%;
  }
  .route-name {
    font-weight: 600;
  }
  .cancel-route {
    background: #5a4a35;
    color: #fff;
    padding: 0.5rem;
  }

  /* Empty / waiting */
  .empty {
    color: var(--chrome-faint);
    font-size: 0.85rem;
    margin: 0;
  }
  .waiting-text {
    color: var(--chrome-faint);
    font-size: 0.9rem;
    text-align: center;
    margin: 0;
  }

  /* Last round + reopen tickets */
  .last-round-banner {
    background: #c0392b;
    color: #fff;
    text-align: center;
    padding: 0.5rem;
    font-weight: bold;
    animation: pulse 1s infinite alternate;
  }
  .reopen-tickets {
    display: block;
    width: calc(100% - 2rem);
    margin: 0.5rem 1rem 0;
    background: #7d3c98;
    color: #fff;
    padding: 0.75rem;
    font-size: 1rem;
  }

  .pay-label {
    font-size: 0.85rem;
    color: var(--chrome-muted);
    margin: 0.25rem 0;
  }
  .pay-invalid {
    font-size: 0.8rem;
    color: var(--danger);
    margin: 0.25rem 0 0;
  }
  /* Ticket flash (§8.2) */
  .ticket-flash {
    position: fixed;
    bottom: 4.5rem;
    left: 1rem;
    right: 1rem;
    background: #1e8449;
    color: #fff;
    text-align: center;
    padding: 0.6rem;
    border-radius: 8px;
    font-size: 0.85rem;
    font-weight: 600;
    z-index: 50;
    animation: flash-in 0.3s ease-out;
  }
  @keyframes flash-in {
    from {
      opacity: 0;
      transform: translateY(10px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  /* Banners */
  .disconnected-banner {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    background: #c0392b;
    color: #fff;
    text-align: center;
    padding: 0.5rem;
    font-weight: bold;
    font-size: 0.85rem;
    z-index: 50;
  }
  .error-banner {
    position: fixed;
    /* In alto: in basso coprirebbe la mano e i pulsanti di conferma. */
    top: 0.5rem;
    left: 0.5rem;
    right: 0.5rem;
    background: #c0392b;
    color: #fff;
    text-align: center;
    padding: 0.6rem;
    font-size: 0.9rem;
    border: none;
    border-radius: 8px;
    z-index: 60;
  }

  /* ── Landscape (telefono orizzontale): esperienza "tavolo da gioco" ──
     3 colonne: biglietti a sinistra, mappa al centro (con la mano a ventaglio
     docked in basso), pesca carte + azioni a destra. Solo CSS, niente JS. */
  @media (orientation: landscape) {
    .cv-game {
      --col-l: 184px;
      --col-r: 214px;
      height: 100dvh;
      display: flex;
      flex-direction: column;
      overflow: hidden;
      padding-bottom: 0;
      position: relative; /* containing block della mano overlay */
    }
    .header {
      position: static;
      padding-top: 0.3rem;
      padding-bottom: 0.3rem;
    }
    .status-bar {
      padding: 0.25rem 1rem;
      font-size: 0.8rem;
    }
    .last-round-banner {
      padding: 0.25rem;
      font-size: 0.8rem;
    }
    /* La plancia è sempre visibile: il toggle resta per il portrait */
    .map-toggle {
      display: none;
    }
    .cv-main {
      flex: 1;
      min-height: 0;
      flex-direction: row;
      gap: 0.4rem;
      padding: 0.4rem 0.5rem;
      padding-left: calc(0.5rem + env(safe-area-inset-left));
      padding-right: calc(0.5rem + env(safe-area-inset-right));
    }
    .cv-map {
      order: 0;
      flex: 1;
      min-width: 0;
      height: auto;
      margin: 0;
    }
    .cv-map.map-hidden {
      display: block; /* landscape: niente collasso */
    }
    /* La mappa compatta ha min-height:220px — in landscape deve poter
       scendere sotto per non invadere la fascia della mano */
    .cv-map :global(.map-container) {
      min-height: 0;
    }
    /* Colonna biglietti: la lista scorre dentro la colonna, la carta di pesca resta in alto */
    .cv-left {
      order: 0;
      flex: 0 0 var(--col-l);
      min-height: 0;
      margin: 0;
    }
    /* Colonna destra: pesca carte in alto, azioni sotto, senza scroll finché basta lo spazio */
    .cv-right {
      order: 0;
      flex: 0 0 var(--col-r);
      overflow-y: auto;
      min-height: 0;
      gap: 0.4rem;
      margin: 0;
    }
    .cv-right .section {
      padding: 0.4rem;
    }
    .cv-right .section h3 {
      font-size: 0.8rem;
      margin-bottom: 0.35rem;
    }
    .cv-right .section h4 {
      font-size: 0.75rem;
    }
    .cv-right .action-btn {
      padding: 0.35rem 0.4rem;
      font-size: 0.78rem;
    }
    .cv-right .actions-hint {
      font-size: 0.72rem;
    }
    .cv-right .route-choices {
      padding: 0.5rem;
      gap: 4px;
    }
    .cv-right .route-option {
      padding: 0.45rem 0.6rem;
      font-size: 0.85rem;
    }
    /* Mano = ventaglio orizzontale ancorato in basso a SINISTRA,
       come carte tenute nel pugno sul bordo del tavolo: il
       centro e il nord della mappa restano scoperti. Fuori dal flusso
       → la mappa arriva in fondo; cattura il puntatore solo sulla sua
       area (fit-content + max-width per non coprire la rail a destra). */
    .hand-dock {
      position: absolute;
      left: calc(0.5rem + env(safe-area-inset-left) + var(--col-l) + 0.8rem);
      bottom: calc(0.5rem + env(safe-area-inset-bottom));
      z-index: 20;
      width: fit-content;
      max-width: calc(100% - var(--col-l) - var(--col-r) - 2.6rem - env(safe-area-inset-left) - env(safe-area-inset-right));
      display: flex;
      align-items: center;
      gap: 0.5rem;
      /* I margini del flusso portrait si applicherebbero come offset
         anche all'absolute: azzerati. */
      margin: 0;
      padding: 0.25rem 0.6rem 0.3rem;
      background: rgba(23, 16, 8, 0.88);
      border-color: rgba(232, 168, 62, 0.35);
      border-radius: 12px;
    }
    /* Niente titolo nel dock: il conteggio è sui badge per colore */
    .hand-dock h3 {
      display: none;
    }
    .hand-dock .hand-fan {
      padding: 0.05rem 0.1rem;
      /* visible: la carta toccata si solleva sopra il ventaglio. Con la
         sovrapposizione a -18px 9 colori stanno in ~360px. */
      overflow: visible;
    }
    .hand-dock :global(.train-card) {
      width: 56px;
    }
    /* Ventaglio orizzontale compatto. La sovrapposizione lascia scoperta una
       striscia di almeno ~28px per carta: quanto basta per il badge del
       conteggio in alto a sinistra (max 9 colori → ~350px). */
    .hand-dock .fan-card + .fan-card {
      margin-left: -18px;
    }
    .hand-dock .fan-card:hover {
      transform: rotate(var(--rot, 0deg)) translateY(-4px);
    }
    /* Banner/transient: ancorati in alto, la mano è ancorata in basso */
    .disconnected-banner {
      top: 0;
      bottom: auto;
    }
    .error-banner {
      top: 2.25rem;
      bottom: auto;
    }
    .ticket-flash {
      top: 4.5rem;
      bottom: auto;
    }
  }

  /* Stazioni (Europe) */
  .build-station {
    background: #8a5a14;
    color: #fff;
  }
  .city-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 40vh;
    overflow-y: auto;
  }

  /* Dialog tunnel (Europe): blocca la schermata finché si decide */
  .tunnel-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.75);
    display: flex;
    z-index: 100;
    overflow-y: auto;
    padding: 0.75rem;
  }
  .tunnel-dialog.wide {
    max-width: 600px;
  }
  .tunnel-dialog {
    background: var(--chrome-bg-1);
    border: 2px solid var(--gold);
    border-radius: 14px;
    padding: 1rem;
    width: 100%;
    max-width: 420px;
    /* Centrato se c'è spazio; se è più alto dello schermo scorre l'overlay (margin auto non taglia in alto) */
    margin: auto;
    box-shadow: var(--shadow-board);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .tunnel-dialog h2 {
    margin: 0;
    font-size: 1.1rem;
    color: var(--gold);
  }
  .tunnel-nopay {
    margin: 0;
    color: var(--chrome-muted);
    font-size: 0.9rem;
  }
  .tunnel-giveup {
    padding: 0.75rem;
    font-size: 1rem;
  }
</style>