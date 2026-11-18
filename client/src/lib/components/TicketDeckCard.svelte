<script lang="ts">
  // Piccola carta "biglietto" (stile tagliando di TicketCard) con il numero di biglietti
  // ancora pescabili: la TV la mostra accanto al segnapunti. Solo visualizzazione.
  let { count }: {
    /** Biglietti rimasti nel mazzo (`publicStore.ticketDeckSize`). */
    count: number;
  } = $props();
</script>

<div
  class="ticket-deck"
  class:empty={count === 0}
  role="img"
  aria-label="{count} {count === 1 ? 'biglietto' : 'biglietti'} nel mazzo"
>
  <span class="td-icon" aria-hidden="true">🎫</span>
  <span class="td-label" aria-hidden="true">Biglietti</span>
  <span class="td-count" aria-hidden="true">{count}</span>
</div>

<style>
  /* Tagliando con bordo tratteggiato e un secondo foglio dietro (mazzo). */
  .ticket-deck {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px;
    min-height: 34px;
    box-sizing: border-box;
    color: var(--board-ink);
    background:
      radial-gradient(circle at 15% 20%, rgba(255, 255, 255, 0.5), transparent 45%),
      linear-gradient(160deg, var(--board-1), var(--board-2));
    border: 1.5px solid var(--board-edge);
    border-radius: 9px;
    box-shadow:
      2px 2px 0 #efe3c4,
      4px 4px 0 rgba(109, 87, 49, 0.85),
      var(--card-shadow);
    margin: 0 4px 4px 0;
    flex: none;
  }
  .ticket-deck::before {
    content: '';
    position: absolute;
    inset: 3px;
    border: 1px dashed rgba(109, 87, 49, 0.5);
    border-radius: 6px;
    pointer-events: none;
  }
  .ticket-deck.empty {
    opacity: 0.45;
    filter: grayscale(0.7);
  }
  .td-icon {
    font-size: 1.05rem;
    line-height: 1;
  }
  .td-label {
    font-size: 0.62rem;
    font-weight: 800;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    line-height: 1.1;
  }
  /* Numero come sul mazzo delle carte: pillola dorata in linea, a destra dell'etichetta. */
  .td-count {
    min-width: 1.4em;
    text-align: center;
    background: var(--gold);
    color: #453416;
    font-weight: 800;
    font-size: 0.85rem;
    line-height: 1;
    padding: 0.25rem 0.4rem;
    border-radius: 999px;
    border: 1.5px solid rgba(69, 52, 22, 0.65);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.35);
  }
</style>
