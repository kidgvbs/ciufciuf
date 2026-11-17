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
  import { routePoints } from '../map-utils';
  import { mostCommonColor, paymentCards, paymentCovered, suggestLocos } from '../payment';
  import { rulesetHasStations } from '../scores';
  import {
    addedColors,
    countByColor,
    flyCard,
    isFreshDrawSource,
    rectOf,
    type DrawSource,
  } from '../draw-animation';
  import type { InboundAction, CardColor, TicketDeckCount, TunnelReveal } from '../proto';
  import { passengerChoices, passengerColorCss, passengerColorName, passengerPicks } from '../passengers';
  import TicketChoiceDialog from './TicketChoiceDialog.svelte';
  import TicketDrawPicker from './TicketDrawPicker.svelte';
  import MapSvg from './MapSvg.svelte';
  import TrainCard from './TrainCard.svelte';
  import TicketCard from './TicketCard.svelte';
  import PaymentPanel from './PaymentPanel.svelte';
  import TunnelCards from './TunnelCards.svelte';
  import { gameMap } from '../game-map.svelte.ts';

  const mapData = $derived(gameMap.state.data);
  const ticketMap = $derived(new Map((mapData?.tickets ?? []).map((t) => [t.id, t])));
  const routeMap = $derived(new Map((mapData?.routes ?? []).map((r) => [r.id, r])));
  const cityMap = $derived(new Map((mapData?.cities ?? []).map((c) => [c.id, c])));

  function cityName(id: string): string {
    return cityMap.get(id)?.name ?? id;
  }

  const CARD_COLORS: CardColor[] = [
    'RED', 'ORANGE', 'YELLOW', 'GREEN', 'BLUE', 'PURPLE', 'BLACK', 'WHITE',
  ];

  /** Colore carta corrispondente al colore della rotta (null = grigia). */
  function routeColorToCard(color: string): CardColor | null {
    const m: Record<string, CardColor> = {
      Red: 'RED', Orange: 'ORANGE', Yellow: 'YELLOW', Green: 'GREEN',
      Blue: 'BLUE', Purple: 'PURPLE', Black: 'BLACK', White: 'WHITE',
    };
    return m[color] ?? null;
  }

  /** Nome italiano del colore di una rotta ("Grigia" se non ha un colore carta). */
  function routeColorName(color: string): string {
    const card = routeColorToCard(color);
    return card ? CARD_COLOR_NAME[card] : 'Grigia';
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
  let choosingRoute = $state(false);
  // Carta della mano toccata: mostrata ingrandita in primo piano
  let focusedColor = $state<CardColor | null>(null);
  // Mappa sul telefono: visibile di default, collassabile
  let showMap = $state(true);
  // Claim: percorso principale = tap sulla mappa; lista testuale = fallback
  let claimTab = $state<'map' | 'list'>('map');

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

  // Stato pannello pagamento rotta (l'utente sceglie colore + locomotive)
  let payingRouteId = $state<string | null>(null);
  let payColor = $state<CardColor | null>(null);
  let payLocos = $state(0);

  // Stazione (Europe): modalità "costruisci", città scelta (mappa o lista)
  let buildingStation = $state(false);
  let stationCity = $state<string | null>(null);
  let stationTab = $state<'map' | 'list'>('map');
  let mapEl = $state<HTMLDivElement>();
  let stationPayEl = $state<HTMLDivElement>();

  // Flash "biglietto completato" (§8.2): salta il primo sync (snapshot/reconnect)
  let knownCompleted: Set<string> | null = null;
  let ticketFlash = $state<string[]>([]);
  let flashTimer: ReturnType<typeof setTimeout> | null = null;

  // Il dialog dei biglietti si apre alla comparsa di un'offerta nuova; "Annulla" lo tiene chiuso
  // finché l'offerta non cambia. Quando l'offerta sparisce la chiusura manuale viene dimenticata.
  const offerId = $derived(offerKey(offered));
  let dismissedOfferId = $state('');
  const ticketDialogOpen = $derived(isDialogOpen(offerId, dismissedOfferId));
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

  // Pesca con le quote per mazzo (Germany): il bottone apre la scelta corti/lunghi.
  const ticketDraw = $derived(legalMoves?.ticket_draw ?? null);
  let pickingTickets = $state(false);
  $effect(() => {
    if (!ticketDraw || !canAct) pickingTickets = false;
  });

  function drawTickets() {
    if (ticketDraw) {
      pickingTickets = true;
      return;
    }
    sendAction({
      actionType: 'DRAW_TICKETS',
      player_id: playerId,
      client_seq: nextClientSeq(playerId),
    });
  }

  // Anti doppio tap sul picker: il draft iniziale lo lascia visibile fino all'offerta.
  let drawingTickets = $state(false);
  const drawLatch = createDrawLatch((pending) => (drawingTickets = pending));
  $effect(() => {
    // Tutte le dipendenze vanno lette prima: niente corto circuito che ne nasconda una.
    const hasOffer = offered.length > 0;
    const error = connectionStore.lastError;
    const canDraw = !!ticketDraw && canAct;
    if (hasOffer || !canDraw) drawLatch.release();
    else drawLatch.releaseOnError(error);
  });

  function drawTicketsWith(counts: TicketDeckCount[]) {
    const sent = drawLatch.submit(
      () =>
        sendAction({
          actionType: 'DRAW_TICKETS',
          player_id: playerId,
          counts,
          client_seq: nextClientSeq(playerId),
        }),
      connectionStore.lastError,
    );
    if (sent) pickingTickets = false;
  }

  // ── Pagamento rotta ────────────────────────────────────────────
  // L'utente sceglie SOLO colore e numero di locomotive (client scemo);
  // il server valida il pagamento (errore mostrato come banner).

  function handCount(color: CardColor): number {
    return hand.filter((c) => c === color).length;
  }

  const locoInHand = $derived(handCount('LOCOMOTIVE'));
  const payingRoute = $derived(
    payingRouteId ? (routeMap.get(payingRouteId) ?? null) : null,
  );
  const isGrayRoute = $derived(payingRoute !== null && routeColorToCard(payingRoute.color) === null);
  /** Locomotive obbligatorie (traghetto); 0 sulle rotte normali. */
  const payFerries = $derived(payingRoute?.ferries ?? 0);
  const maxLocos = $derived(payingRoute ? Math.min(locoInHand, payingRoute.len) : 0);
  const payColoredNeeded = $derived(payingRoute ? payingRoute.len - payLocos : 0);
  // Guardia di UX: la mano copre la scelta (locomotive dei traghetti comprese); il server arbitra.
  const payValid = $derived(
    payingRoute !== null &&
      paymentCovered(
        {
          total: payingRoute.len,
          minLocos: payFerries,
          colorCount: payColor ? handCount(payColor) : 0,
          locoInHand,
        },
        payLocos,
      ),
  );
  const paySummaryText = $derived.by(() => {
    const parts: string[] = [];
    if (payColoredNeeded > 0 && payColor) parts.push(`${payColoredNeeded}× ${CARD_COLOR_NAME[payColor]}`);
    if (payLocos > 0) parts.push(`${payLocos}× ${CARD_COLOR_NAME.LOCOMOTIVE}`);
    return parts.length > 0 ? parts.join(' + ') : '—';
  });

  /** Apre il pannello pagamento per una rotta, con default sensati. */
  function startPayment(routeId: string) {
    const route = routeMap.get(routeId);
    if (!route) return;
    payingRouteId = routeId;
    // Default: colore della rotta, o (grigia) il colore più presente in mano.
    const preset = routeColorToCard(route.color) ?? mostCommonColor(hand);
    payColor = preset;
    // Default locomotive: quelle necessarie a coprire la rotta col colore scelto
    // (almeno quelle obbligatorie del traghetto).
    payLocos = suggestLocos({
      total: route.len,
      minLocos: route.ferries ?? 0,
      colorCount: preset ? handCount(preset) : 0,
      locoInHand,
    });
  }

  function cancelPayment() {
    payingRouteId = null;
  }

  // Tap-to-claim dalla mappa: rotta claimabile → pannello pagamento.
  // Rotte non claimabili: niente (client scemo — la verità è legalMoves).
  function onRouteTap(routeId: string) {
    if (!legalMoves || buildingStation) return;
    if (legalMoves.routes.includes(routeId)) {
      startPayment(routeId);
    }
  }

  // Passeggeri (Germany 2017): negli estremi con più colori il giocatore sceglie quale prendere.
  let passengerPicked = $state<Record<string, string>>({});
  const passengerChoiceList = $derived(
    payingRoute ? passengerChoices(publicStore.passengers, payingRoute) : [],
  );
  $effect(() => {
    void payingRouteId;
    passengerPicked = {};
  });

  function confirmPayment() {
    const route = payingRoute;
    if (!route || !payValid) return;
    const cards = paymentCards(hand, payColor, route.len, payLocos);

    const sent = sendAction({
      actionType: 'CLAIM_ROUTE',
      player_id: playerId,
      route_id: route.id,
      cards_discarded: cards,
      passengers: passengerPicks(passengerChoiceList, passengerPicked),
      client_seq: nextClientSeq(playerId),
    });
    // Se l'azione non è partita il pannello resta aperto per riprovare.
    if (!sent) return;

    payingRouteId = null;
    choosingRoute = false;
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

  // La modalità si chiude da sola se non è più possibile costruire (turno passato, carte cambiate).
  $effect(() => {
    if (!canAct || stationCities.length === 0) {
      buildingStation = false;
      stationCity = null;
    } else if (stationCity !== null && !stationCities.includes(stationCity)) {
      stationCity = null;
    }
  });

  // Scelta della città sulla mappa: la mappa (sopra il rail in portrait) torna in vista;
  // scelta la città, torna in vista il pannello di pagamento.
  $effect(() => {
    if (buildingStation && stationCity === null && stationTab === 'map') revealEl(mapEl);
  });
  $effect(() => {
    if (buildingStation && stationCity !== null) revealEl(stationPayEl);
  });

  // Da tastiera il pannello di pagamento viene dopo la mappa nel DOM: scelta la città il focus
  // lo raggiunge, senza dover scorrere con Tab tutte le città. Con il tocco basta lo scroll.
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
    choosingRoute = false;
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
      if (offered.length > 0) return 'Scegli i biglietti iniziali';
      return ticketDraw ? 'Scegli quanti biglietti corti e lunghi pescare' : 'Attendi che tutti scelgano...';
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
</script>

<div class="cv-game" bind:this={rootEl}>
  <!-- Sfondo inerte mentre il dialog dei biglietti è aperto. -->
  <div class="cv-bg" inert={ticketDialogOpen || tunnelMove !== null}>
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
    <button class="btn leave-btn" onclick={onLeave}> Esci </button>
  </div>

  <!-- Status -->
  <div class="status-bar" class:my-turn={isMyTurn && canAct} role="status" aria-live="polite">
    {statusLabel}
  </div>

  {#if publicStore.lastRound && gameState !== 'GAME_OVER'}
    <div class="last-round-banner">⚠️ ULTIMO GIRO</div>
  {/if}

  {#if offerId !== '' && !ticketDialogOpen}
    <button class="btn reopen-tickets" onclick={() => (dismissedOfferId = '')}>
      🎫 Devi scegliere i biglietti ({offered.length})
    </button>
  {/if}

  {#if gameState === 'INITIAL_DRAFT' && ticketDraw && offered.length === 0}
    <div class="section initial-ticket-draw">
      <TicketDrawPicker
        draw={ticketDraw}
        busy={drawBusy || drawingTickets}
        showRemaining={false}
        title="Biglietti iniziali"
        onConfirm={drawTicketsWith}
      />
    </div>
  {/if}

  <!-- Toggle mappa: solo portrait (in landscape la plancia è sempre visibile) -->
  <button
    class="map-toggle"
    onclick={() => (showMap = !showMap)}
    aria-expanded={showMap}
  >
    🗺️ Mappa {showMap ? '▾' : '▸'}
  </button>

  <!-- Layout principale: mappa dominante + colonna laterale (biglietti/azioni).
       Portrait: colonna sotto la mappa via CSS; landscape: affiancata. -->
  <div class="cv-main">
    <!-- Mappa con tap-to-claim: nessun glow sulle claimabili
         (l'utente le trova da solo; il tap alla cieca resta
         comunque sicuro perché onRouteTap filtra su legalMoves).
         highlightTickets: solo i biglietti pinnati, mai le claimabili. -->
    <div class="cv-map" class:map-hidden={!showMap} bind:this={mapEl}>
      {#if buildingStation && stationCity === null && stationTab === 'map'}
        <div class="map-hint" role="status">🏠 Tocca una città evidenziata per la stazione</div>
      {/if}
      <MapSvg
        compact
        selectedRouteId={payingRouteId}
        onRouteTap={onRouteTap}
        highlightTickets={highlightTickets}
        selectableCityIds={buildingStation ? stationCities : []}
        selectedCityId={stationCity}
        onCityTap={onCityTap}
      />
    </div>

    <div class="cv-rail">
      <!-- Tickets -->
      <div class="section">
        <h3>Biglietti ({tickets.length})</h3>
        {#if tickets.length > 0}
          <div class="ticket-list">
            {#each tickets as t (t.id)}
              {@const desc = ticketMap.get(t.id)}
              <TicketCard
                from={desc ? cityName(desc.from) : t.id}
                to={desc ? cityName(desc.to) : ''}
                points={t.points}
                long={desc?.deck === 'long'}
                completed={t.completed}
                active={pinnedTicketIds.includes(t.id) && !completedTicketIds.has(t.id)}
                onclick={() => toggleTicketHighlight(t.id)}
              />
            {/each}
          </div>
        {:else}
          <p class="empty">Nessun biglietto</p>
        {/if}
      </div>

  <!-- Actions -->
  {#if canAct && legalMoves}
    <div class="section actions">
      <h3>Azioni</h3>

      {#if legalMoves.can_draw_deck}
        <button onclick={(e) => drawDeck(e.currentTarget)} class="btn action-btn draw-deck" disabled={drawBusy}>
          🃏 Pesca dal mazzo
        </button>
      {/if}

      {#if legalMoves.draw_face_up.length > 0}
        <div class="face-up-actions">
          {#each legalMoves.draw_face_up as idx}
            {@const card = publicStore.visibleDeck[idx]}
            <button
              onclick={(e) => drawFaceUp(idx, e.currentTarget)}
              class="btn action-btn face-up"
              disabled={drawBusy}
              aria-label={card ? `Pesca carta ${CARD_COLOR_NAME[card]}` : 'Pesca carta scoperta'}
            >
              {#if card}
                <TrainCard color={card} />
              {/if}
            </button>
          {/each}
        </div>
      {/if}

      {#if legalMoves.can_draw_tickets}
        {#if ticketDraw && pickingTickets}
          <TicketDrawPicker
            draw={ticketDraw}
            busy={drawBusy || drawingTickets}
            title="Pesca biglietti"
            onConfirm={drawTicketsWith}
            onCancel={() => (pickingTickets = false)}
          />
        {:else}
          <button onclick={drawTickets} class="btn action-btn draw-tickets">
            🎫 Pesca biglietti
          </button>
        {/if}
      {/if}

      {#if legalMoves.routes.length > 0}
        {#if !choosingRoute}
          <button
            onclick={() => { choosingRoute = true; cancelStation(); }}
            class="btn action-btn claim-route"
          >
            🛤️ Reclama rotta ({legalMoves.routes.length})
          </button>
        {:else if !payingRoute}
          <div class="route-choices">
            <div class="claim-tabs">
              <button
                class="btn claim-tab"
                class:active={claimTab === 'map'}
                onclick={() => (claimTab = 'map')}
              >
                🗺️ Mappa
              </button>
              <button
                class="btn claim-tab"
                class:active={claimTab === 'list'}
                onclick={() => (claimTab = 'list')}
              >
                ☰ Lista
              </button>
            </div>
            {#if claimTab === 'list'}
              <h4>Scegli la rotta</h4>
              {#each legalMoves.routes as rid (rid)}
                {@const r = routeMap.get(rid)}
                {#if r}
                  <button onclick={() => startPayment(rid)} class="btn route-option">
                    <div class="route-name">{cityName(r.from)} → {cityName(r.to)}</div>
                    <div class="route-meta">
                      {routeColorName(r.color)} · {r.len} 🚂 · ⭐{routePoints(r.len)}{#if r.ferries} · ⛴ {r.ferries} loco{/if}{#if r.tunnel} · tunnel{/if}
                    </div>
                  </button>
                {/if}
              {/each}
            {/if}
            <button
              onclick={() => (choosingRoute = false)}
              class="btn cancel-route"
            >
              Annulla
            </button>
          </div>
        {:else if payingRoute}
          {@const r = payingRoute}
          <div class="route-choices payment">
            <h4>
              Paga: {cityName(r.from)} → {cityName(r.to)}
              ({r.len} 🚂)
            </h4>
            {#if payFerries > 0}
              <p class="pay-label">⛴ Traghetto: servono almeno {payFerries} locomotive.</p>
            {/if}
            {#if r.tunnel}
              <p class="pay-label">Tunnel: dopo il pagamento si rivelano alcune carte e potresti dover pagare carte extra (o rinunciare).</p>
            {/if}

            {#if isGrayRoute}
              <p class="pay-label">Colore (rotta grigia):</p>
              <div class="pay-colors">
                {#each CARD_COLORS as c}
                  <button
                    class="pay-swatch"
                    class:selected={payColor === c}
                    aria-pressed={payColor === c}
                    disabled={handCount(c) === 0}
                    onclick={() => { payColor = c; }}
                    aria-label="Paga con {CARD_COLOR_NAME[c]} ({handCount(c)} in mano)"
                  >
                    <TrainCard color={c} count={handCount(c)} small />
                  </button>
                {/each}
              </div>
            {:else if payColor}
              <p class="pay-label">
                Colore: <strong>{CARD_COLOR_NAME[payColor]}</strong>
                (hai {handCount(payColor)})
              </p>
            {/if}

            <div class="pay-locos">
              <span class="pay-label">Locomotive (hai {locoInHand}):</span>
              <div class="locos-stepper">
                <button
                  class="btn"
                  onclick={() => (payLocos = Math.max(payFerries, payLocos - 1))}
                  aria-label="Meno locomotive"
                  disabled={payLocos <= payFerries}
                >−</button>
                <span class="locos-value">{payLocos}</span>
                <button
                  class="btn"
                  onclick={() => (payLocos = Math.min(maxLocos, payLocos + 1))}
                  aria-label="Più locomotive"
                  disabled={payLocos >= maxLocos}
                >+</button>
              </div>
            </div>

            <p class="pay-summary">Paghi: {paySummaryText}</p>
            {#each passengerChoiceList as choice (choice.cityId)}
              <p class="pay-label">Passeggero da prendere a {cityName(choice.cityId)}:</p>
              <div class="pay-colors" role="radiogroup" aria-label="Passeggero a {cityName(choice.cityId)}">
                {#each choice.passengers as p (p.color)}
                  {@const selected = (passengerPicked[choice.cityId] ?? choice.passengers[0]?.color) === p.color}
                  <button
                    class="passenger-swatch"
                    class:selected
                    role="radio"
                    aria-checked={selected}
                    onclick={() => (passengerPicked = { ...passengerPicked, [choice.cityId]: p.color })}
                    aria-label="Passeggero {passengerColorName(p.color)} ({p.count})"
                  >
                    <span class="passenger-dot" style="background:{passengerColorCss(p.color)}"></span>
                    ×{p.count}
                  </button>
                {/each}
              </div>
            {/each}
            {#if !payValid}
              <p class="pay-invalid">
                {payFerries > locoInHand
                  ? `Servono almeno ${payFerries} locomotive (ne hai ${locoInHand})`
                  : 'Carte insufficienti per questa combinazione'}
              </p>
            {/if}

            <button
              class="btn confirm-pay"
              onclick={confirmPayment}
              disabled={!payValid}
            >
              ✅ Conferma e reclama
            </button>
            <button onclick={cancelPayment} class="btn cancel-route">
              Indietro
            </button>
          </div>
        {/if}
      {/if}
      {#if stationCities.length > 0}
        {#if !buildingStation}
          <button onclick={startStation} class="btn action-btn build-station">
            🏠 Costruisci stazione ({stationCost} {stationCost === 1 ? 'carta' : 'carte'})
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
              <p class="pay-label">Tocca una città evidenziata sulla mappa (in alto).</p>
            {/if}
            <button onclick={cancelStation} class="btn cancel-route">Annulla</button>
          </div>
        {:else}
          <div bind:this={stationPayEl}>
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
          </div>
          <button onclick={cancelStation} class="btn cancel-route">Annulla</button>
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
              class:focused={focusedColor === color}
              style="--rot:{rot}deg"
              aria-label="{CARD_COLOR_NAME[color]}: {count} in mano"
              onclick={() => (focusedColor = focusedColor === color ? null : color)}
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

  <!-- Ticket choice dialog -->
  {#if ticketDialogOpen}
    <TicketChoiceDialog
      offeredTickets={offered}
      choice={ticketChoice}
      onConfirm={confirmTicketChoice}
      onCancel={cancelTicketChoice}
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

  /* Tickets (fila di tagliandi) */
  .ticket-list {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .ticket-list > :global(.ticket) {
    flex: 1 1 150px;
    max-width: 220px;
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
  .draw-deck {
    background: #2471a3;
    color: #fff;
  }
  .draw-tickets {
    background: #7d3c98;
    color: #fff;
  }
  .claim-route {
    background: #c0392b;
    color: #fff;
  }
  .face-up-actions {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    justify-content: center;
  }
  .face-up {
    flex: 0 0 auto;
    padding: 3px;
    background: var(--chrome-bg-2);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  /* ── Layout principale: mappa + colonna laterale ─────────────────
     Portrait (default): colonna — mappa (collassabile) sopra, rail sotto. */
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
  .cv-rail {
    display: flex;
    flex-direction: column;
  }
  .cv-rail .section {
    margin: 0.75rem 1rem 0;
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
  .route-meta {
    font-size: 0.8rem;
    color: var(--chrome-muted);
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

  /* Payment panel */
  .route-choices.payment {
    border: 1px solid var(--gold);
  }
  .pay-label {
    font-size: 0.85rem;
    color: var(--chrome-muted);
    margin: 0.25rem 0;
  }
  .pay-colors {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .pay-swatch {
    background: none;
    border: 2px solid transparent;
    border-radius: 10px;
    padding: 2px;
    cursor: pointer;
    min-width: 44px;
    min-height: 44px;
  }
  .pay-swatch.selected {
    border-color: var(--gold);
    background: rgba(232, 168, 62, 0.18);
  }
  .pay-swatch:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }
  .passenger-swatch {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: none;
    color: inherit;
    border: 2px solid transparent;
    border-radius: 10px;
    padding: 4px 8px;
    cursor: pointer;
    min-width: 44px;
    min-height: 44px;
  }
  .passenger-swatch.selected {
    border-color: var(--gold);
    background: rgba(232, 168, 62, 0.18);
  }
  .passenger-dot {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 2px solid rgba(0, 0, 0, 0.45);
  }
  .pay-locos {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    margin-top: 0.25rem;
  }
  .locos-stepper {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .locos-stepper button {
    width: 36px;
    height: 36px;
    background: #5a4a35;
    color: #fff;
    font-size: 1.2rem;
  }
  .locos-value {
    min-width: 2rem;
    text-align: center;
    font-weight: bold;
    font-size: 1.1rem;
  }
  .pay-summary {
    font-size: 0.85rem;
    color: var(--gold);
    margin: 0.25rem 0 0;
  }
  .pay-invalid {
    font-size: 0.8rem;
    color: var(--danger);
    margin: 0.25rem 0 0;
  }
  .confirm-pay {
    background: #1e8449;
    color: #fff;
    padding: 0.75rem;
    font-size: 1rem;
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
     Mappa dominante a sinistra, biglietti+azioni in colonna a destra,
     mano a ventaglio docked in basso. Solo CSS, niente JS. */
  @media (orientation: landscape) {
    .cv-game {
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
    /* Rail compatta (~250px): resta leggibile ma discreta */
    .cv-rail {
      flex: 0 0 250px;
      overflow-y: auto;
      min-height: 0;
      padding-bottom: 0.4rem;
    }
    .cv-rail .section {
      margin: 0 0 0.4rem;
      padding: 0.4rem;
    }
    .cv-rail .section h3 {
      font-size: 0.8rem;
      margin-bottom: 0.35rem;
    }
    .cv-rail .section h4 {
      font-size: 0.75rem;
    }
    .cv-rail .action-btn {
      padding: 0.5rem;
      font-size: 0.85rem;
    }
    .cv-rail .face-up {
      padding: 2px;
    }
    .cv-rail .route-choices {
      padding: 0.5rem;
      gap: 4px;
    }
    .cv-rail .route-option {
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
      left: calc(0.5rem + env(safe-area-inset-left));
      bottom: calc(0.5rem + env(safe-area-inset-bottom));
      z-index: 20;
      width: fit-content;
      max-width: min(64%, 500px);
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
    align-items: center;
    justify-content: center;
    z-index: 100;
    overflow-y: auto;
    padding: 0.75rem;
  }
  .tunnel-dialog {
    background: var(--chrome-bg-1);
    border: 2px solid var(--gold);
    border-radius: 14px;
    padding: 1rem;
    width: 100%;
    max-width: 420px;
    max-height: 100%;
    overflow-y: auto;
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