<script lang="ts">
  // Popup di reclamo rotta (controller): si apre toccando una rotta evidenziata sulla mappa.
  // Mostra la tratta, la scelta del colore (rotte grigie), le locomotive e, se servono, i
  // passeggeri; ✅ invia il reclamo, ❌ annulla.
  //
  // Client scemo: la rotta è già nelle mosse legali del server; qui c'è solo la guardia di UX
  // «la mano copre il pagamento scelto», l'arbitro resta il server.

  import type { CardColor, PassengerCount, PassengerPick } from '../proto';
  import type { RouteData } from '../map-utils';
  import { routePoints } from '../map-utils';
  import { mountainClaimNote } from '../mountains';
  import { CARD_COLOR_CSS, CARD_COLOR_NAME } from '../colors';
  import { createOverlayDismiss } from '../overlay-dismiss';
  import { passengerColorCss, passengerColorName, passengerPicks } from '../passengers';
  import {
    PLAIN_CARD_COLORS,
    countCards,
    findRoutePayment,
    locoBounds,
    maxGroups,
    mostCommonColor,
    routeColorToCard,
    routePayableWith,
    routePaymentCards,
    routePaymentRules,
    routePaymentValid,
    suggestRoutePayment,
    type RulesetPayment,
  } from '../payment';
  import TrainCard from './TrainCard.svelte';

  let {
    route,
    cityName,
    hand,
    selectedColor = null,
    ruleset = null,
    mountainPointsPerTrain = null,
    passengerChoiceList = [],
    onConfirm,
    onCancel,
  }: {
    route: RouteData;
    cityName: (cityId: string) => string;
    hand: CardColor[];
    /** Carta toccata nella mano: colore preselezionato se vale per questa rotta. */
    selectedColor?: CardColor | null;
    /** Regole di pagamento del ruleset (locomotive ammesse, carte qualsiasi); assenti = classiche. */
    ruleset?: RulesetPayment | null;
    /** Punti per ogni treno scartato sulle rotte di montagna (dal catalogo); `null` = non noti. */
    mountainPointsPerTrain?: number | null;
    /** Estremi con passeggeri di più colori (Germany 2017). */
    passengerChoiceList?: Array<{ cityId: string; passengers: PassengerCount[] }>;
    onConfirm: (cards: CardColor[], passengers: PassengerPick[] | undefined) => void;
    onCancel: () => void;
  } = $props();

  const overlay = createOverlayDismiss(() => onCancel());

  const routeCard = $derived(routeColorToCard(route.color));
  const isGray = $derived(routeCard === null);
  const rules = $derived(routePaymentRules(route, ruleset));
  /** Locomotive obbligatorie (traghetto); 0 sulle rotte normali. */
  const ferries = $derived(rules.ferries);
  /** Carte qualsiasi che pagano uno spazio (gruppo); `null` = nessuna sostituzione. */
  const groupSize = $derived(rules.groupSize);
  /** «⛰ −k treni, +2k punti» sulle rotte di montagna; `null` sulle altre. */
  const mountainNote = $derived(mountainClaimNote(route, mountainPointsPerTrain));
  const locoInHand = $derived(countCards(hand, 'LOCOMOTIVE'));
  const bounds = $derived(locoBounds(rules, locoInHand));
  const groupLimit = $derived(maxGroups(rules, hand));

  // Scelte dell'utente; finché non tocca nulla valgono i default calcolati qui sotto.
  let colorPick = $state<CardColor | null>(null);
  let locosPick = $state<number | null>(null);
  let groupsPick = $state<number | null>(null);
  let passengerPicked = $state<Record<string, string>>({});

  // Colore preselezionato: quello della carta toccata in mano se paga questa rotta, altrimenti il più
  // presente; se con quello la mano non copre il pagamento (gruppi di carte qualsiasi) il primo che lo copre.
  const preferredColor = $derived.by(() => {
    const preferred =
      selectedColor !== null &&
      selectedColor !== 'LOCOMOTIVE' &&
      routePayableWith(route, hand, selectedColor, ruleset)
        ? selectedColor
        : mostCommonColor(hand);
    if (preferred === null || findRoutePayment(rules, hand, preferred) !== null) return preferred;
    return (
      PLAIN_CARD_COLORS.find((c) => countCards(hand, c) > 0 && findRoutePayment(rules, hand, c) !== null) ??
      preferred
    );
  });
  const payColor = $derived<CardColor | null>(routeCard ?? colorPick ?? preferredColor);
  // Toccato il jolly in mano (e nessun colore scelto dopo): si parte pagando con sole locomotive.
  const allLocosPayment = $derived(
    selectedColor === 'LOCOMOTIVE' && colorPick === null ? findRoutePayment(rules, hand, null) : null,
  );
  const suggestion = $derived(allLocosPayment ?? suggestRoutePayment(rules, hand, payColor));
  const payLocos = $derived(locosPick ?? suggestion.locos);
  const payGroups = $derived(groupsPick ?? suggestion.groups);
  const coloredNeeded = $derived(route.len - payLocos - payGroups);
  const payValid = $derived(
    routePaymentValid(rules, hand, payColor, { locos: payLocos, groups: payGroups }),
  );
  const summaryText = $derived.by(() => {
    const parts: string[] = [];
    if (coloredNeeded > 0 && payColor) parts.push(`${coloredNeeded}× ${CARD_COLOR_NAME[payColor]}`);
    if (payLocos > 0) parts.push(`${payLocos}× ${CARD_COLOR_NAME.LOCOMOTIVE}`);
    if (payGroups > 0) parts.push(`${payGroups}× ${groupSize} carte qualsiasi`);
    return parts.length > 0 ? parts.join(' + ') : '—';
  });

  function pickColor(c: CardColor) {
    colorPick = c;
    // Locomotive e gruppi tornano al default per il nuovo colore.
    locosPick = null;
    groupsPick = null;
  }

  function confirm() {
    if (!payValid) return;
    onConfirm(
      routePaymentCards(rules, hand, payColor, { locos: payLocos, groups: payGroups }),
      passengerPicks(passengerChoiceList, passengerPicked),
    );
  }

  // Focus nel popup all'apertura (lo sfondo è inerte) e ripristino alla chiusura.
  let dialogEl = $state<HTMLDivElement>();
  $effect(() => {
    const previous = document.activeElement;
    dialogEl?.focus();
    return () => {
      if (previous instanceof HTMLElement) previous.focus();
    };
  });

  // Il popup si apre su un `pointerup` della mappa: il `click` che il browser genera subito dopo
  // cade sull'elemento sotto il dito, cioè sul popup appena comparso (anche su ✅) o sull'overlay
  // (che lo chiuderebbe). Per un attimo né il popup né l'overlay ricevono tocchi.
  let armed = $state(false);
  $effect(() => {
    const t = setTimeout(() => (armed = true), 400);
    return () => clearTimeout(t);
  });

  function onKeydown(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Escape') onCancel();
  }
</script>

<div
  class="cd-overlay"
  class:armed
  role="presentation"
  onpointerdown={overlay.onpointerdown}
  onpointerup={overlay.onpointerup}
  onclick={overlay.onclick}
>
  <div
    class="cd"
    role="dialog"
    aria-modal="true"
    aria-labelledby="claim-title"
    tabindex="-1"
    bind:this={dialogEl}
    onkeydown={onKeydown}
  >
    <header class="cd-head">
      <h2 id="claim-title">{cityName(route.from)} → {cityName(route.to)}</h2>
      <p class="cd-meta">
        <span class="cd-chip" style="background:{routeCard ? CARD_COLOR_CSS[routeCard] : '#9aa0a6'}" aria-hidden="true"></span>
        {routeCard ? CARD_COLOR_NAME[routeCard] : 'Grigia'} · {route.len} 🚂 · ⭐{routePoints(route.len)}
      </p>
    </header>

    {#if route.anyCardsPerSpace !== undefined}
      <p class="cd-note">
        Ogni spazio si può pagare con {route.anyCardsPerSpace} carte qualsiasi (locomotive comprese).
      </p>
    {:else if ferries > 0 && groupSize !== null}
      <p class="cd-note">
        ⛴ Traghetto: servono {ferries} locomotive, ognuna sostituibile da {groupSize} carte qualsiasi.
      </p>
    {:else if ferries > 0}
      <p class="cd-note">⛴ Traghetto: servono almeno {ferries} locomotive.</p>
    {/if}
    {#if route.tunnel}
      <p class="cd-note">
        Tunnel: dopo il pagamento si rivelano alcune carte e potresti dover pagare carte extra (o rinunciare).
      </p>
    {/if}
    {#if mountainNote}
      <p class="cd-note">{mountainNote} (oltre alle carte, dalla riserva)</p>
    {/if}

    <div class="cd-body">
      <section class="cd-col">
        {#if isGray}
          <p class="cd-label">Colore (rotta grigia):</p>
          <div class="cd-colors">
            {#each PLAIN_CARD_COLORS as c (c)}
              <button
                type="button"
                class="cd-swatch"
                class:selected={payColor === c}
                aria-pressed={payColor === c}
                disabled={countCards(hand, c) === 0}
                onclick={() => pickColor(c)}
                aria-label="Paga con {CARD_COLOR_NAME[c]} ({countCards(hand, c)} in mano)"
              >
                <TrainCard color={c} count={countCards(hand, c)} small />
              </button>
            {/each}
          </div>
        {:else if payColor}
          <p class="cd-label">
            Colore: <strong>{CARD_COLOR_NAME[payColor]}</strong> (hai {countCards(hand, payColor)})
          </p>
        {/if}

        {#if rules.singleLoco || ferries > 0}
          <div class="cd-locos">
            <span class="cd-label">Jolly (hai {locoInHand}):</span>
            <div class="cd-stepper">
              <button
                type="button"
                class="btn"
                onclick={() => (locosPick = Math.max(bounds.min, payLocos - 1))}
                aria-label="Meno locomotive"
                disabled={payLocos <= bounds.min}
              >−</button>
              <span class="cd-locos-value">{payLocos}</span>
              <button
                type="button"
                class="btn"
                onclick={() => (locosPick = Math.min(bounds.max, payLocos + 1))}
                aria-label="Più locomotive"
                disabled={payLocos >= bounds.max || coloredNeeded <= 0}
              >+</button>
            </div>
          </div>
        {:else}
          <p class="cd-label">Le locomotive non sono ammesse su questa rotta{groupSize !== null ? ' (solo nei gruppi di carte qualsiasi)' : ''}.</p>
        {/if}
        {#if groupSize !== null}
          <div class="cd-locos">
            <span class="cd-label">Gruppi di {groupSize} carte qualsiasi:</span>
            <div class="cd-stepper">
              <button
                type="button"
                class="btn"
                onclick={() => (groupsPick = Math.max(0, payGroups - 1))}
                aria-label="Meno gruppi di carte qualsiasi"
                disabled={payGroups <= 0}
              >−</button>
              <span class="cd-locos-value">{payGroups}</span>
              <button
                type="button"
                class="btn"
                onclick={() => (groupsPick = Math.min(groupLimit, payGroups + 1))}
                aria-label="Più gruppi di carte qualsiasi"
                disabled={payGroups >= groupLimit || coloredNeeded <= 0}
              >+</button>
            </div>
          </div>
        {/if}
        <p class="cd-summary">Paghi: {summaryText}</p>
        {#if !payValid}
          <p class="cd-invalid">
            {ferries > locoInHand && groupSize === null
              ? `Servono almeno ${ferries} locomotive (ne hai ${locoInHand})`
              : 'Carte insufficienti per questa combinazione'}
          </p>
        {/if}
      </section>

      {#if passengerChoiceList.length > 0}
        <section class="cd-col">
          {#each passengerChoiceList as choice (choice.cityId)}
            <p class="cd-label">Passeggero da prendere a {cityName(choice.cityId)}:</p>
            <div class="cd-colors" role="radiogroup" aria-label="Passeggero a {cityName(choice.cityId)}">
              {#each choice.passengers as p (p.color)}
                {@const selected = (passengerPicked[choice.cityId] ?? choice.passengers[0]?.color) === p.color}
                <button
                  type="button"
                  class="cd-passenger"
                  class:selected
                  role="radio"
                  aria-checked={selected}
                  onclick={() => (passengerPicked = { ...passengerPicked, [choice.cityId]: p.color })}
                  aria-label="Passeggero {passengerColorName(p.color)} ({p.count})"
                >
                  <span class="cd-dot" style="background:{passengerColorCss(p.color)}"></span>
                  ×{p.count}
                </button>
              {/each}
            </div>
          {/each}
        </section>
      {/if}
    </div>

    <footer class="cd-actions">
      <button type="button" class="cd-btn cd-cancel" onclick={onCancel} aria-label="Annulla">
        <span class="cd-emoji" aria-hidden="true">❌</span>
        <span>Annulla</span>
      </button>
      <button
        type="button"
        class="cd-btn cd-confirm"
        onclick={confirm}
        disabled={!payValid}
        aria-label="Conferma e reclama"
      >
        <span class="cd-emoji" aria-hidden="true">✅</span>
        <span>Reclama</span>
      </button>
    </footer>
  </div>
</div>

<style>
  .cd-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.72);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    padding: 0.5rem;
    box-sizing: border-box;
  }
  .cd {
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    width: 100%;
    max-width: 600px;
    max-height: 100%;
    overflow-y: auto;
    padding: 0.8rem;
    background: var(--chrome-bg-1);
    border: 2px solid var(--gold);
    border-radius: 14px;
    box-shadow: var(--shadow-board);
    color: var(--chrome-text);
    outline: none;
  }
  /* Subito dopo l'apertura i tocchi (il click fantasma del tap sulla mappa) non raggiungono né
     il popup né l'overlay: `pointer-events` si eredita dai figli. */
  .cd-overlay:not(.armed) {
    pointer-events: none;
  }
  .cd-head h2 {
    margin: 0;
    font-size: 1.1rem;
    color: var(--gold);
  }
  .cd-meta {
    margin: 0.15rem 0 0;
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.85rem;
    color: var(--chrome-muted);
  }
  .cd-chip {
    width: 14px;
    height: 14px;
    border-radius: 4px;
    border: 1px solid rgba(255, 255, 255, 0.5);
  }
  .cd-note {
    margin: 0;
    font-size: 0.8rem;
    color: var(--chrome-muted);
  }
  .cd-body {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem 1rem;
  }
  .cd-col {
    flex: 1 1 240px;
    min-width: 0;
  }
  .cd-label {
    margin: 0.2rem 0;
    font-size: 0.85rem;
    color: var(--chrome-muted);
  }
  .cd-colors {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .cd-swatch {
    background: none;
    border: 2px solid transparent;
    border-radius: 10px;
    padding: 2px;
    cursor: pointer;
    min-width: 44px;
    min-height: 44px;
  }
  .cd-swatch.selected {
    border-color: var(--gold);
    background: rgba(232, 168, 62, 0.18);
  }
  .cd-swatch:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }
  .cd-passenger {
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
  .cd-passenger.selected {
    border-color: var(--gold);
    background: rgba(232, 168, 62, 0.18);
  }
  .cd-dot {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 2px solid rgba(0, 0, 0, 0.45);
  }
  .cd-locos {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    margin-top: 0.3rem;
  }
  .cd-stepper {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .cd-stepper button {
    width: 40px;
    height: 40px;
    background: #5a4a35;
    color: #fff;
    font-size: 1.2rem;
  }
  .cd-locos-value {
    min-width: 2rem;
    text-align: center;
    font-weight: bold;
    font-size: 1.15rem;
  }
  .cd-summary {
    margin: 0.3rem 0 0;
    font-size: 0.85rem;
    color: var(--gold);
  }
  .cd-invalid {
    margin: 0.25rem 0 0;
    font-size: 0.8rem;
    color: var(--danger);
  }
  .cd-actions {
    display: flex;
    gap: 0.6rem;
  }
  .cd-btn {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    min-height: 52px;
    border: none;
    border-radius: 12px;
    color: #fff;
    font-family: inherit;
    font-size: 1rem;
    font-weight: 700;
    cursor: pointer;
  }
  .cd-emoji {
    font-size: 1.7rem;
    line-height: 1;
  }
  .cd-confirm {
    background: #1e8449;
  }
  .cd-cancel {
    background: #8c2f25;
  }
  .cd-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .cd-btn:focus-visible,
  .cd-swatch:focus-visible,
  .cd-passenger:focus-visible {
    outline: 2px solid var(--gold);
    outline-offset: 2px;
  }
</style>
