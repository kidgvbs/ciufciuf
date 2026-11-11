<script lang="ts">
  // Carte rivelate da un tentativo di tunnel (Europe), condivise da TV e controller.
  // Evidenzia quelle che aumentano il costo (stesso colore del tunnel o locomotiva; solo
  // locomotiva se il colore è LOCOMOTIVE). Il costo extra mostrato è quello del server.
  import type { TunnelReveal } from '../proto';
  import { CARD_COLOR_NAME } from '../colors';
  import { tunnelCardRaisesCost } from '../payment';
  import TrainCard from './TrainCard.svelte';

  let { reveal }: { reveal: TunnelReveal } = $props();
</script>

<div class="tunnel-cards">
  <div class="cards-row" role="list" aria-label="Carte rivelate">
    {#each reveal.cards as card, i (i)}
      {@const raises = tunnelCardRaisesCost(card, reveal.color)}
      <div class="revealed" class:raises class:neutral={!raises} role="listitem"
        aria-label="{CARD_COLOR_NAME[card]}{raises ? ': costo extra' : ''}">
        <TrainCard color={card} small />
        {#if raises}<span class="plus" aria-hidden="true">+1</span>{/if}
      </div>
    {:else}
      <p class="none">Nessuna carta rivelata</p>
    {/each}
  </div>
  <p class="summary">
    Colore del tunnel: <strong>{CARD_COLOR_NAME[reveal.color]}</strong>
    · Costo extra: <strong>{reveal.extra_cost}</strong>
  </p>
</div>

<style>
  .tunnel-cards {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
  }
  .cards-row {
    display: flex;
    gap: 10px;
    justify-content: center;
    flex-wrap: wrap;
    padding: 6px 4px 0;
  }
  .revealed {
    position: relative;
    border-radius: 10px;
    padding: 2px;
  }
  .revealed.raises {
    outline: 3px solid var(--gold);
    background: rgba(232, 168, 62, 0.25);
    box-shadow: 0 0 12px rgba(232, 168, 62, 0.6);
  }
  .revealed.neutral {
    opacity: 0.55;
  }
  .plus {
    position: absolute;
    top: -8px;
    right: -8px;
    background: var(--gold);
    color: #453416;
    font-weight: 800;
    font-size: 0.75rem;
    border-radius: 999px;
    padding: 0.05rem 0.4rem;
  }
  .none {
    margin: 0;
    color: var(--chrome-faint);
    font-size: 0.85rem;
  }
  .summary {
    margin: 0;
    font-size: 0.9rem;
    color: var(--chrome-muted);
  }
  .summary strong {
    color: var(--gold);
  }
</style>
