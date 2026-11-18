<script lang="ts">
  // Colonna biglietti del controller: in alto la «carta biglietto» per pescarne di nuovi,
  // sotto i miei biglietti divisi in «Da completare» e «Completati» (attenuati).
  // Lo stato di completamento arriva dal server (`TicketInfo.completed`).
  import type { TicketInfo } from '../proto';
  import { gameMap } from '../game-map.svelte.ts';
  import { ticketView } from '../ticket-offer';
  import TicketCard from './TicketCard.svelte';

  let {
    tickets,
    pinnedIds,
    onToggle,
    canDraw,
    busy = false,
    onDraw,
    remaining = null,
  }: {
    tickets: TicketInfo[];
    /** Biglietti evidenziati sulla mappa. */
    pinnedIds: string[];
    onToggle: (ticketId: string) => void;
    /** `legalMoves.can_draw_tickets`: la carta è attiva solo se il server lo consente. */
    canDraw: boolean;
    /** Azione già in volo: la carta resta spenta. */
    busy?: boolean;
    onDraw: () => void;
    /** Biglietti rimasti nel mazzo: badge sulla carta, nascosto se `null`. */
    remaining?: number | null;
  } = $props();

  const mapData = $derived(gameMap.state.data);
  const ticketMap = $derived(new Map((mapData?.tickets ?? []).map((t) => [t.id, t])));
  const cityMap = $derived(new Map((mapData?.cities ?? []).map((c) => [c.id, c])));

  function cityName(id: string): string {
    return cityMap.get(id)?.name ?? id;
  }

  const todo = $derived(tickets.filter((t) => !t.completed));
  const done = $derived(tickets.filter((t) => t.completed));
  const drawDisabled = $derived(!canDraw || busy);
</script>

<div class="ticket-col">
  <button
    type="button"
    class="draw-ticket"
    class:inactive={!canDraw}
    disabled={drawDisabled}
    onclick={onDraw}
    aria-label="Pesca biglietti"
  >
    <span class="dt-icon" aria-hidden="true">🎫</span>
    <span class="dt-label">Pesca<br />biglietti</span>
    {#if remaining !== null}
      <span class="dt-count" aria-label="{remaining} biglietti nel mazzo">{remaining}</span>
    {/if}
  </button>

  <div class="tc-scroll">
    {#if tickets.length === 0}
      <p class="empty">Nessun biglietto</p>
    {:else}
      <h3>Da completare ({todo.length})</h3>
      {#if todo.length > 0}
        <div class="ticket-list">
          {#each todo as t (t.id)}
            {@const desc = ticketMap.get(t.id)}
            {@const view = ticketView(t, desc, cityName)}
            <TicketCard
              compact
              from={view.from}
              to={view.to}
              points={view.points}
              rows={view.rows}
              long={desc?.deck === 'long'}
              active={pinnedIds.includes(t.id)}
              onclick={() => onToggle(t.id)}
            />
          {/each}
        </div>
      {:else}
        <p class="empty">Tutti completati</p>
      {/if}

      {#if done.length > 0}
        <h3 class="done-title">Completati ({done.length})</h3>
        <div class="ticket-list done-list">
          {#each done as t (t.id)}
            {@const desc = ticketMap.get(t.id)}
            {@const view = ticketView(t, desc, cityName)}
            <TicketCard
              compact
              completed
              from={view.from}
              to={view.to}
              points={view.points}
              rows={view.rows}
              long={desc?.deck === 'long'}
            />
          {/each}
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .ticket-col {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    min-height: 0;
    background: var(--chrome-bg-1);
    border: 1px solid var(--chrome-line);
    border-radius: 12px;
    padding: 0.5rem;
    box-shadow: var(--shadow-panel);
  }
  .tc-scroll {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    /* Spazio per spunte e badge che sporgono dai tagliandi */
    padding: 6px 6px 8px 2px;
  }
  h3 {
    margin: 0 0 0.35rem;
    font-size: 0.8rem;
    color: var(--gold);
  }
  .done-title {
    margin-top: 0.7rem;
    color: var(--chrome-muted);
  }
  .ticket-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .ticket-list > :global(.ticket) {
    width: 100%;
    box-sizing: border-box;
    cursor: pointer;
  }
  .done-list {
    opacity: 0.55;
  }
  .done-list > :global(.ticket) {
    cursor: default;
  }
  .empty {
    margin: 0;
    color: var(--chrome-faint);
    font-size: 0.8rem;
  }

  /* Carta biglietto: sagoma da tagliando con il bordo tratteggiato e un secondo foglio dietro. */
  .draw-ticket {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 6px 10px;
    min-height: 46px;
    font-family: inherit;
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
    cursor: pointer;
    text-align: left;
  }
  .draw-ticket::before {
    content: '';
    position: absolute;
    inset: 3px;
    border: 1px dashed rgba(109, 87, 49, 0.5);
    border-radius: 6px;
    pointer-events: none;
  }
  .draw-ticket:focus-visible {
    outline: 2px solid var(--gold);
    outline-offset: 3px;
  }
  .draw-ticket:disabled {
    cursor: not-allowed;
  }
  .draw-ticket.inactive,
  .draw-ticket:disabled {
    opacity: 0.4;
    filter: grayscale(0.7);
  }
  .dt-icon {
    font-size: 1.3rem;
    line-height: 1;
  }
  .dt-label {
    font-size: 0.72rem;
    font-weight: 800;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    line-height: 1.15;
  }
  /* Badge dei biglietti rimasti, come il numero sul mazzo delle carte */
  .dt-count {
    position: absolute;
    top: -8px;
    right: -8px;
    min-width: 1.4em;
    text-align: center;
    background: var(--gold);
    color: #453416;
    font-weight: 800;
    font-size: 0.8rem;
    line-height: 1;
    padding: 0.25rem 0.4rem;
    border-radius: 999px;
    border: 2px solid var(--chrome-bg-0);
    box-shadow: 0 2px 5px rgba(0, 0, 0, 0.55);
  }

  @media (orientation: landscape) {
    .ticket-col {
      height: 100%;
      box-sizing: border-box;
      padding: 0.4rem;
      gap: 0.4rem;
    }
  }
</style>
