<script lang="ts">
  // Area «Pesca carte» del controller: le 5 carte scoperte e il mazzo coperto, tutti della
  // stessa misura in una griglia compatta. Le carte sono sempre visibili (anche fuori turno);
  // quelle che il server non consente di pescare (`legal_moves.draw_face_up`, `can_draw_deck`)
  // restano al loro posto ma inattive.
  import type { CardColor } from '../proto';
  import { CARD_COLOR_NAME } from '../colors';
  import TrainCard from './TrainCard.svelte';
  import DeckCard from './DeckCard.svelte';

  /** Carte scoperte sul tavolo. */
  const FACE_UP_SLOTS = 5;

  let {
    visibleDeck,
    deckSize,
    faceUpMoves,
    canDrawDeck,
    busy = false,
    onDrawDeck,
    onDrawFaceUp,
  }: {
    /** `PublicState.visible_deck`: `null` = slot vuoto. */
    visibleDeck: (CardColor | null)[];
    deckSize: number;
    /** `legalMoves.draw_face_up`: indici pescabili adesso. */
    faceUpMoves: number[];
    /** `legalMoves.can_draw_deck`. */
    canDrawDeck: boolean;
    /** Azione già in volo: le carte restano spente. */
    busy?: boolean;
    onDrawDeck: (source: HTMLElement) => void;
    onDrawFaceUp: (index: number, source: HTMLElement) => void;
  } = $props();

  const slots = $derived(
    Array.from({ length: Math.max(FACE_UP_SLOTS, visibleDeck.length) }, (_, i) => visibleDeck[i] ?? null),
  );
</script>

<div class="draw-area">
  <h3>Pesca carte</h3>
  <div class="draw-grid">
    <button
      type="button"
      class="draw-cell deck-cell"
      disabled={!canDrawDeck || busy}
      onclick={(e) => onDrawDeck(e.currentTarget)}
      aria-label="Pesca dal mazzo coperto ({deckSize} carte)"
    >
      <DeckCard count={deckSize} />
    </button>
    {#each slots as card, i (i)}
      {#if card}
        <button
          type="button"
          class="draw-cell"
          disabled={!faceUpMoves.includes(i) || busy}
          onclick={(e) => onDrawFaceUp(i, e.currentTarget)}
          aria-label="Pesca carta {CARD_COLOR_NAME[card]}"
        >
          <TrainCard color={card} />
        </button>
      {:else}
        <div class="draw-cell empty" aria-label="Posto vuoto"></div>
      {/if}
    {/each}
  </div>
</div>

<style>
  .draw-area {
    background: var(--chrome-bg-1);
    border: 1px solid var(--chrome-line);
    border-radius: 12px;
    padding: 0.5rem;
    box-shadow: var(--shadow-panel);
  }
  h3 {
    margin: 0 0 0.35rem;
    font-size: 0.8rem;
    color: var(--gold);
  }
  .draw-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 72px));
    justify-content: center;
    gap: 8px;
  }
  .draw-cell {
    /* reset del <button> */
    display: block;
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: inherit;
    width: 100%;
    cursor: pointer;
    border-radius: var(--card-radius);
    transition: transform 0.12s ease, opacity 0.12s ease;
  }
  .draw-cell:not(:disabled):not(.empty):hover {
    transform: translateY(-3px);
  }
  .draw-cell:focus-visible {
    outline: 2px solid var(--gold);
    outline-offset: 2px;
  }
  /* Inattiva: ben visibile ma spenta (non è il tuo turno o la mossa non è ammessa) */
  .draw-cell:disabled {
    cursor: not-allowed;
    opacity: 0.5;
    filter: saturate(0.6);
  }
  .draw-cell.empty {
    aspect-ratio: 2 / 3;
    border: 1px dashed rgba(232, 168, 62, 0.35);
    background: rgba(0, 0, 0, 0.18);
    cursor: default;
  }
  /* Carte e mazzo riempiono la cella: stessa misura per tutti */
  .draw-cell :global(.train-card),
  .draw-cell :global(.deck-card) {
    width: 100%;
  }
  /* Il rilievo della pila e il numero del mazzo sporgono un po' dalla cella */
  .deck-cell :global(.deck-count) {
    font-size: 0.8rem;
    bottom: -6px;
    right: -6px;
  }
</style>
