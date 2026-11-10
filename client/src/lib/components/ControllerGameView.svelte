<script lang="ts">
  // Vista di gioco per il controller (telefono).
  // Mostra: mano, biglietti, azioni legali, scelta biglietti.

  import { publicStore, privateStore, connectionStore } from '../stores.svelte.ts';
  import { PLAYER_COLOR_CSS, CARD_COLOR_NAME } from '../colors';
  import { errorMessage } from '../errors';
  import { routePoints } from '../map-utils';
  import type { InboundAction, CardColor, TicketInfo, LegalMoves } from '../proto';
  import TicketChoiceDialog from './TicketChoiceDialog.svelte';
  import MapSvg from './MapSvg.svelte';
  import TrainCard from './TrainCard.svelte';
  import TicketCard from './TicketCard.svelte';
  import usaMap from '../data/usa.json';
  import type { MapData } from '../map-utils';

  const mapData = usaMap as MapData;
  const ticketMap = new Map(mapData.tickets.map((t) => [t.id, t]));
  const routeMap = new Map(mapData.routes.map((r) => [r.id, r]));
  const cityMap = new Map(mapData.cities.map((c) => [c.id, c]));

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
  let showTicketDialog = $state(false);
  // Carta della mano toccata: mostrata ingrandita in primo piano
  let focusedColor = $state<CardColor | null>(null);
  let ticketMinKeep = $state(1);
  // Mappa sul telefono (D25): visibile di default, collassabile
  let showMap = $state(true);
  // Claim: percorso principale = tap sulla mappa; lista testuale = fallback
  let claimTab = $state<'map' | 'list'>('map');

  // Biglietti pinnati sulla mappa (Fase 3f→3f+2): MULTI-pin — ogni
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

  // Flash "biglietto completato" (§8.2): salta il primo sync (snapshot/reconnect)
  let knownCompleted: Set<string> | null = null;
  let ticketFlash = $state<string[]>([]);
  let flashTimer: ReturnType<typeof setTimeout> | null = null;

  // Guard degli offered_tickets (E3)
  $effect(() => {
    if (offered.length > 0) {
      showTicketDialog = true;
      ticketMinKeep = gameState === 'INITIAL_DRAFT' ? 2 : 1;
    }
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

  // E8: il banner di errore si auto-chiude dopo pochi secondi. Il cleanup
  // cancella il timer quando arriva un nuovo errore o allo smontaggio.
  $effect(() => {
    if (!connectionStore.lastError) return;
    const t = setTimeout(() => { connectionStore.lastError = null; }, 4000);
    return () => clearTimeout(t);
  });

  // F1: cancella i timer one-shot allo smontaggio del componente
  $effect(() => () => {
    if (flashTimer) clearTimeout(flashTimer);
  });

  // D3: vibrazione quando diventa il mio turno (silenziosa dove non supportata)
  let wasMyTurn = false;
  $effect(() => {
    const now = isMyTurn && canAct;
    if (now && !wasMyTurn) {
      navigator.vibrate?.(200);
    }
    wasMyTurn = now;
  });

  // ── Azioni ─────────────────────────────────────────────────────
  function drawDeck() {
    sendAction({
      actionType: 'DRAW_TRAIN_CARD',
      player_id: playerId,
      source: 'deck',
      client_seq: null,
    });
  }

  function drawFaceUp(index: number) {
    sendAction({
      actionType: 'DRAW_TRAIN_CARD',
      player_id: playerId,
      source: { faceUp: { index } },
      client_seq: null,
    });
  }

  function drawTickets() {
    sendAction({
      actionType: 'DRAW_TICKETS',
      player_id: playerId,
      client_seq: null,
    });
  }

  // ── Pagamento rotta ────────────────────────────────────────────
  // L'utente sceglie SOLO colore e numero di locomotive (client scemo);
  // il server valida il pagamento (E8: errore mostrato come banner).

  function handCount(color: CardColor): number {
    return hand.filter((c) => c === color).length;
  }

  const locoInHand = $derived(handCount('LOCOMOTIVE'));
  const payingRoute = $derived(
    payingRouteId ? (routeMap.get(payingRouteId) ?? null) : null,
  );
  const isGrayRoute = $derived(payingRoute !== null && routeColorToCard(payingRoute.color) === null);
  const maxLocos = $derived(payingRoute ? Math.min(locoInHand, payingRoute.len) : 0);
  const payColoredNeeded = $derived(payingRoute ? payingRoute.len - payLocos : 0);
  const payValid = $derived(
    payingRoute !== null &&
      (payColoredNeeded === 0 ||
        (payColor !== null && handCount(payColor) >= payColoredNeeded)),
  );
  const paySummaryText = $derived.by(() => {
    const parts: string[] = [];
    if (payColoredNeeded > 0 && payColor) parts.push(`${payColoredNeeded}× ${payColor}`);
    if (payLocos > 0) parts.push(`${payLocos}× LOCOMOTIVE`);
    return parts.length > 0 ? parts.join(' + ') : '—';
  });

  /** Apre il pannello pagamento per una rotta, con default sensati. */
  function startPayment(routeId: string) {
    const route = routeMap.get(routeId);
    if (!route) return;
    payingRouteId = routeId;
    // Default: colore della rotta, o (grigia) il colore più presente in mano.
    let preset = routeColorToCard(route.color);
    if (!preset) {
      let bestN = 0;
      for (const c of CARD_COLORS) {
        const n = handCount(c);
        if (n > bestN) { bestN = n; preset = c; }
      }
    }
    payColor = preset;
    // Default locomotive: quelle necessarie a coprire la rotta col colore scelto.
    const have = preset ? handCount(preset) : 0;
    payLocos = Math.min(Math.max(0, route.len - have), Math.min(locoInHand, route.len));
  }

  function cancelPayment() {
    payingRouteId = null;
  }

  // Tap-to-claim dalla mappa (D25): rotta claimabile → pannello pagamento.
  // Rotte non claimabili: niente (client scemo — la verità è legalMoves).
  function onRouteTap(routeId: string) {
    if (!legalMoves) return;
    if (legalMoves.routes.includes(routeId)) {
      startPayment(routeId);
    }
  }

  function confirmPayment() {
    const route = payingRoute;
    if (!route || !payValid) return;
    const cards: CardColor[] = [];
    const coloredNeeded = route.len - payLocos;
    if (payColor) {
      for (const c of hand) {
        if (c === payColor && cards.length < coloredNeeded) cards.push(c);
      }
    }
    let locos = 0;
    for (const c of hand) {
      if (c === 'LOCOMOTIVE' && locos < payLocos) {
        cards.push(c);
        locos++;
      }
    }

    const sent = sendAction({
      actionType: 'CLAIM_ROUTE',
      player_id: playerId,
      route_id: route.id,
      cards_discarded: cards,
      client_seq: null,
    });
    // Se l'azione non è partita il pannello resta aperto per riprovare.
    if (!sent) return;

    payingRouteId = null;
    choosingRoute = false;
  }

  function confirmTicketChoice(keep: string[]) {
    const sent = sendAction({
      actionType: 'CHOOSE_TICKETS',
      player_id: playerId,
      keep,
      client_seq: null,
    });
    if (sent) showTicketDialog = false;
  }

  function cancelTicketChoice() {
    showTicketDialog = false;
  }

  // ── Label stato ────────────────────────────────────────────────
  // ($derived.by: il valore È il testo, non una funzione che lo calcola)
  const statusLabel = $derived.by(() => {
    if (gameState === 'WAITING_FOR_PLAYERS') return 'In attesa di giocatori...';
    if (gameState === 'INITIAL_DRAFT') {
      return offered.length > 0
        ? 'Scegli i biglietti iniziali'
        : 'Attendi che tutti scelgano...';
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
    if (gameState === 'TUNNEL_RESOLUTION') return 'Risoluzione tunnel...';
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

  const showMyScore = $derived(
    gameState === 'GAME_OVER' && publicStore.finalScores,
  );

  function myFinalScore() {
    return publicStore.finalScores?.find((fs) => fs.playerId === playerId);
  }
</script>

<div class="cv-game">
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
      <span title="Carte nel mazzo">🃏 {publicStore.deckSize}</span>
    </div>
    <button class="btn leave-btn" onclick={onLeave}> Esci </button>
  </div>

  <!-- Status -->
  <div class="status-bar" class:my-turn={isMyTurn && canAct}>
    {statusLabel}
  </div>

  {#if publicStore.lastRound && gameState !== 'GAME_OVER'}
    <div class="last-round-banner">⚠️ ULTIMO GIRO</div>
  {/if}

  {#if offered.length > 0 && !showTicketDialog}
    <button class="btn reopen-tickets" onclick={() => (showTicketDialog = true)}>
      🎫 Devi scegliere i biglietti ({offered.length})
    </button>
  {/if}

  <!-- Game over score -->
  {#if showMyScore}
    {@const fs = myFinalScore()}
    {#if fs}
      <div class="section my-score">
        <h3>Il tuo punteggio</h3>
        <div class="score-grid">
          <div class="score-row"><span>Rotte</span><span>{fs.routePoints}</span></div>
          <div class="score-row"><span>Biglietti completati</span><span>+{fs.ticketCompleted}</span></div>
          <div class="score-row"><span>Biglietti falliti</span><span>{fs.ticketFailed}</span></div>
          {#if fs.longestPathBonus > 0}
            <div class="score-row bonus"><span>🏆 Longest Path</span><span>+{fs.longestPathBonus}</span></div>
          {/if}
          <div class="score-row total"><span>Totale</span><span>{fs.total}</span></div>
        </div>
      </div>
    {/if}
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
    <!-- Mappa con tap-to-claim (D25): nessun glow sulle claimabili
         (3f+2: l'utente le trova da solo; il tap alla cieca resta
         comunque sicuro perché onRouteTap filtra su legalMoves).
         highlightTickets: solo i biglietti pinnati, mai le claimabili. -->
    <div class="cv-map" class:map-hidden={!showMap}>
      <MapSvg
        compact
        selectedRouteId={payingRouteId}
        onRouteTap={onRouteTap}
        highlightTickets={highlightTickets}
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
        <button onclick={drawDeck} class="btn action-btn draw-deck" disabled={drawBusy}>
          🃏 Pesca dal mazzo
        </button>
      {/if}

      {#if legalMoves.draw_face_up.length > 0}
        <div class="face-up-actions">
          {#each legalMoves.draw_face_up as idx}
            {@const card = publicStore.visibleDeck[idx]}
            <button
              onclick={() => drawFaceUp(idx)}
              class="btn action-btn face-up"
              disabled={drawBusy}
              aria-label={card ? `Pesca carta ${card}` : 'Pesca carta scoperta'}
            >
              {#if card}
                <TrainCard color={card} />
              {/if}
            </button>
          {/each}
        </div>
      {/if}

      {#if legalMoves.can_draw_tickets}
        <button onclick={drawTickets} class="btn action-btn draw-tickets">
          🎫 Pesca biglietti
        </button>
      {/if}

      {#if legalMoves.routes.length > 0}
        {#if !choosingRoute}
          <button
            onclick={() => (choosingRoute = true)}
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
                      {r.color} · {r.len} 🚂 · ⭐{routePoints(r.len)}
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

            {#if isGrayRoute}
              <p class="pay-label">Colore (rotta grigia):</p>
              <div class="pay-colors">
                {#each CARD_COLORS as c}
                  <button
                    class="pay-swatch"
                    class:selected={payColor === c}
                    disabled={handCount(c) === 0}
                    onclick={() => { payColor = c; }}
                    aria-label="Paga con {c} ({handCount(c)} in mano)"
                  >
                    <TrainCard color={c} count={handCount(c)} small />
                  </button>
                {/each}
              </div>
            {:else if payColor}
              <p class="pay-label">
                Colore: <strong>{payColor}</strong>
                (hai {handCount(payColor)})
              </p>
            {/if}

            <div class="pay-locos">
              <span class="pay-label">Locomotive (hai {locoInHand}):</span>
              <div class="locos-stepper">
                <button
                  class="btn"
                  onclick={() => (payLocos = Math.max(0, payLocos - 1))}
                  disabled={payLocos <= 0}
                >−</button>
                <span class="locos-value">{payLocos}</span>
                <button
                  class="btn"
                  onclick={() => (payLocos = Math.min(maxLocos, payLocos + 1))}
                  disabled={payLocos >= maxLocos}
                >+</button>
              </div>
            </div>

            <p class="pay-summary">Paghi: {paySummaryText}</p>
            {#if !payValid}
              <p class="pay-invalid">Carte insufficienti per questa combinazione</p>
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
        <div class="hand-fan">
          {#each hbc as [color, count], i}
            {@const rot = (i - (hbc.length - 1) / 2) * 6}
            <button
              type="button"
              class="fan-card"
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
    <button
      class="error-banner"
      aria-live="assertive"
      onclick={() => (connectionStore.lastError = null)}
    >
      ❌ {errorMessage(connectionStore.lastError)}
    </button>
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

  <!-- Ticket choice dialog -->
  {#if showTicketDialog}
    <TicketChoiceDialog
      offeredTickets={offered}
      minKeep={ticketMinKeep}
      onConfirm={confirmTicketChoice}
      onCancel={cancelTicketChoice}
    />
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
    margin: 0.75rem 1rem 0;
    height: 240px;
  }
  .cv-map.map-hidden {
    display: none;
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

  /* My score (game over) */
  .my-score {
    border: 2px solid var(--gold);
  }
  .score-grid {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .score-row {
    display: flex;
    justify-content: space-between;
    padding: 0.25rem 0.5rem;
    font-size: 0.9rem;
  }
  .score-row.total {
    font-weight: bold;
    font-size: 1.1rem;
    color: var(--gold);
    border-top: 1px solid var(--chrome-line);
    margin-top: 4px;
    padding-top: 6px;
  }
  .score-row.bonus {
    color: #2ecc71;
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
  }
  .pay-swatch.selected {
    border-color: var(--gold);
    background: rgba(232, 168, 62, 0.18);
  }
  .pay-swatch:disabled {
    opacity: 0.3;
    cursor: not-allowed;
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
    /* Rail compatta (~250px): resta leggibile ma discreta (Fase 3f) */
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
    /* Mano = ventaglio orizzontale (3f) ancorato in basso a SINISTRA
       (3f+2), come carte tenute nel pugno sul bordo del tavolo: il
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
         anche all'absolute: azzerati (lezione F1 review 3f). */
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
</style>