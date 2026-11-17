<script lang="ts">
  // Pesca dei biglietti con la scelta del mazzo (Germany): quanti corti e quanti lunghi,
  // con il totale fissato dal server. Con due mazzi basta spostare un biglietto dall'uno
  // all'altro; con più mazzi ogni riga sposta dal mazzo precedente con biglietti.
  import type { TicketDeckCount, TicketDrawMoves } from '../proto';
  import { canConfirmQuotas, deckLabel, defaultQuotas, moveQuota, quotasPayload } from '../ticket-draw';

  let { draw, busy = false, title, onConfirm, onCancel }: {
    draw: TicketDrawMoves;
    busy?: boolean;
    title: string;
    onConfirm: (counts: TicketDeckCount[]) => void;
    onCancel?: () => void;
  } = $props();

  let picked = $state<number[] | null>(null);
  const quotas = $derived(picked ?? defaultQuotas(draw));
  const valid = $derived(canConfirmQuotas(draw, quotas));

  /** Un biglietto in più nel mazzo `i`, preso dal primo altro mazzo che ne ha. */
  function more(i: number) {
    const from = quotas.findIndex((q, j) => j !== i && q > 0);
    if (from >= 0) picked = moveQuota(draw, quotas, from, i);
  }

  /** Un biglietto in meno nel mazzo `i`, dato al primo altro mazzo che ne ha ancora. */
  function less(i: number) {
    const to = quotas.findIndex((q, j) => j !== i && q < (draw.available[j]?.count ?? 0));
    if (to >= 0) picked = moveQuota(draw, quotas, i, to);
  }
</script>

<div class="ticket-draw-picker">
  <h4>{title}</h4>
  <p class="pay-label">
    Peschi {draw.total} {draw.total === 1 ? 'biglietto' : 'biglietti'}: scegli da quali mazzi.
  </p>
  {#each draw.available as deck, i (deck.deck)}
    <div class="deck-row">
      <span class="deck-name">{deckLabel(deck.deck)} <small>({deck.count} rimasti)</small></span>
      <div class="locos-stepper">
        <button
          class="btn"
          onclick={() => less(i)}
          disabled={(quotas[i] ?? 0) <= 0}
          aria-label="Un biglietto {deckLabel(deck.deck).toLowerCase()} in meno"
        >−</button>
        <span class="locos-value">{quotas[i]}</span>
        <button
          class="btn"
          onclick={() => more(i)}
          disabled={(quotas[i] ?? 0) >= deck.count || quotas.every((q, j) => j === i || q === 0)}
          aria-label="Un biglietto {deckLabel(deck.deck).toLowerCase()} in più"
        >+</button>
      </div>
    </div>
  {/each}
  <button
    class="btn confirm-pay"
    disabled={!valid || busy}
    onclick={() => onConfirm(quotasPayload(draw, quotas))}
  >
    🎫 Pesca
  </button>
  {#if onCancel}
    <button class="btn cancel-route" onclick={onCancel}>Annulla</button>
  {/if}
</div>

<style>
  .ticket-draw-picker {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .deck-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .deck-name small {
    opacity: 0.7;
  }
  .locos-stepper {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .locos-value {
    min-width: 1.5rem;
    text-align: center;
    font-weight: 700;
  }
</style>
