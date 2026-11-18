<script lang="ts">
  // Pannello di pagamento carte: colore + locomotive (stesso schema del pagamento rotta).
  // Usato per la stazione (colore libero) e per le carte extra del tunnel (colore del tunnel
  // fisso, `LOCOMOTIVE` = solo locomotive).
  //
  // Client scemo: il totale arriva dal server (`station_cost`, `extra_cost`); qui c'è solo la
  // guardia di UX "la mano copre la scelta", l'arbitro resta il server.
  import type { CardColor } from '../proto';
  import { CARD_COLOR_NAME } from '../colors';
  import {
    PLAIN_CARD_COLORS,
    countCards,
    mostCommonColor,
    paymentCards,
    paymentCovered,
    suggestLocos,
    tunnelPayColor,
  } from '../payment';
  import TrainCard from './TrainCard.svelte';

  let {
    total,
    hand,
    fixedColor,
    allowLocos = true,
    title,
    confirmLabel,
    busy = false,
    onConfirm,
    backLabel,
    onBack,
  }: {
    /** Carte da pagare. */
    total: number;
    hand: CardColor[];
    /** Colore imposto (tunnel); assente = a scelta del giocatore (stazione). */
    fixedColor?: CardColor;
    /** Le locomotive possono pagare queste carte (`locomotives.usableOn`); default sì. */
    allowLocos?: boolean;
    title: string;
    confirmLabel: string;
    /** Azione già in volo: il bottone di conferma resta spento. */
    busy?: boolean;
    onConfirm: (cards: CardColor[]) => void;
    backLabel?: string;
    onBack?: () => void;
  } = $props();

  let colorPick = $state<CardColor | null>(null);
  let locosPick = $state<number | null>(null);

  const color = $derived<CardColor | null>(
    fixedColor !== undefined ? tunnelPayColor(fixedColor) : (colorPick ?? mostCommonColor(hand)),
  );
  const locoInHand = $derived(countCards(hand, 'LOCOMOTIVE'));
  const colorCount = $derived(color ? countCards(hand, color) : 0);
  // Locomotive utilizzabili: quelle in mano se ammesse su questa rotta (`allowLocos`), altrimenti nessuna.
  const spec = $derived({ total, minLocos: 0, colorCount, locoInHand: allowLocos ? locoInHand : 0 });
  const locos = $derived(locosPick ?? suggestLocos(spec));
  const maxLocos = $derived(spec.locoInHand === 0 ? 0 : Math.min(spec.locoInHand, total));
  const valid = $derived(paymentCovered(spec, locos));
  const locoOnly = $derived(fixedColor !== undefined && color === null);
  const coloredNeeded = $derived(total - locos);

  const summary = $derived.by(() => {
    const parts: string[] = [];
    if (coloredNeeded > 0 && color) parts.push(`${coloredNeeded}× ${CARD_COLOR_NAME[color]}`);
    if (locos > 0) parts.push(`${locos}× ${CARD_COLOR_NAME.LOCOMOTIVE}`);
    return parts.length > 0 ? parts.join(' + ') : '—';
  });

  function confirm() {
    if (!valid || busy) return;
    onConfirm(paymentCards(hand, color, total, locos));
  }
</script>

<div class="pay-panel">
  <h4>{title}</h4>

  {#if fixedColor === undefined}
    <p class="pay-label">Colore:</p>
    <div class="pay-colors">
      {#each PLAIN_CARD_COLORS as c}
        <button
          class="pay-swatch"
          class:selected={color === c}
          aria-pressed={color === c}
          disabled={countCards(hand, c) === 0}
          onclick={() => { colorPick = c; locosPick = null; }}
          aria-label="Paga con {CARD_COLOR_NAME[c]} ({countCards(hand, c)} in mano)"
        >
          <TrainCard color={c} count={countCards(hand, c)} small />
        </button>
      {/each}
    </div>
  {:else if locoOnly}
    <p class="pay-label">Solo locomotive (hai {locoInHand})</p>
  {:else if color}
    <p class="pay-label">
      Colore: <strong>{CARD_COLOR_NAME[color]}</strong> (hai {colorCount}) o locomotive
    </p>
  {/if}

  {#if !locoOnly}
    <div class="pay-locos">
      <span class="pay-label">Locomotive (hai {locoInHand}):</span>
      <div class="locos-stepper">
        <button
          class="btn"
          onclick={() => (locosPick = Math.max(0, locos - 1))}
          aria-label="Meno locomotive"
          disabled={locos <= 0}
        >−</button>
        <span class="locos-value">{locos}</span>
        <button
          class="btn"
          onclick={() => (locosPick = Math.min(maxLocos, locos + 1))}
          aria-label="Più locomotive"
          disabled={locos >= maxLocos}
        >+</button>
      </div>
    </div>
  {/if}

  <p class="pay-summary">Paghi: {summary}</p>
  {#if !valid}
    <p class="pay-invalid">Carte insufficienti per questa combinazione</p>
  {/if}

  <button class="btn confirm-pay" onclick={confirm} disabled={!valid || busy}>
    {confirmLabel}
  </button>
  {#if onBack}
    <button class="btn back-pay" onclick={onBack}>{backLabel ?? 'Indietro'}</button>
  {/if}
</div>

<style>
  .pay-panel {
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--chrome-bg-0);
    border: 1px solid var(--gold);
    border-radius: 10px;
    padding: 0.75rem;
  }
  h4 {
    margin: 0.25rem 0;
    font-size: 0.85rem;
    color: var(--gold);
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
  .back-pay {
    background: #5a4a35;
    color: #fff;
    padding: 0.5rem;
  }
</style>
