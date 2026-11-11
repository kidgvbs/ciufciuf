<script lang="ts">
  // Dialog scelta biglietti (initial draft o pescata mid-game). Come in partita, un biglietto
  // selezionato appare sulla mappa con la linea tratteggiata del suo colore; deselezionato, sparisce.

  import type { TicketInfo } from '../proto';
  import { gameMap } from '../game-map.svelte.ts';
  import { offeredTicketPins, ticketPinColor } from '../map-utils';
  import MapSvg from './MapSvg.svelte';
  import TicketCard from './TicketCard.svelte';

  const mapData = $derived(gameMap.state.data);
  const ticketMap = $derived(new Map((mapData?.tickets ?? []).map((t) => [t.id, t])));
  const cityMap = $derived(new Map((mapData?.cities ?? []).map((c) => [c.id, c])));

  function cityName(id: string): string {
    return cityMap.get(id)?.name ?? id;
  }

  let { offeredTickets, minKeep, onConfirm, onCancel }: {
    offeredTickets: TicketInfo[];
    minKeep: number;
    onConfirm: (keepIds: string[]) => void;
    onCancel: () => void;
  } = $props();

  let selected = $state<Set<string>>(new Set());

  const pins = $derived(offeredTicketPins(offeredTickets, mapData?.tickets ?? [], selected));

  function toggle(id: string) {
    const next = new Set(selected);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    selected = next;
  }

  const canConfirm = $derived(selected.size >= minKeep);

  function confirm() {
    onConfirm([...selected]);
  }

  const FOCUSABLE = 'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])';

  // Focus iniziale nel dialog e ripristino dell'elemento che lo aveva prima.
  let dialogEl = $state<HTMLDivElement>();
  $effect(() => {
    const previous = document.activeElement;
    const first = dialogEl?.querySelector<HTMLElement>('.ticket-item') ?? dialogEl?.querySelector<HTMLElement>(FOCUSABLE);
    (first ?? dialogEl)?.focus();
    return () => {
      if (previous instanceof HTMLElement) previous.focus();
    };
  });

  // Escape annulla; Tab e Maiusc+Tab restano dentro il dialog.
  function onDialogKeydown(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Escape') {
      onCancel();
      return;
    }
    if (e.key !== 'Tab' || !dialogEl) return;
    const items = [...dialogEl.querySelectorAll<HTMLElement>(FOCUSABLE)];
    const first = items[0];
    const last = items[items.length - 1];
    if (!first || !last) {
      e.preventDefault();
      dialogEl.focus();
      return;
    }
    const active = document.activeElement;
    if (e.shiftKey && (active === first || active === dialogEl)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first.focus();
    }
  }
</script>

<div
  class="dialog-overlay"
  role="presentation"
  onclick={(e: MouseEvent) => { if (e.target === e.currentTarget) onCancel(); }}
  onkeydown={(e: KeyboardEvent) => { if (e.key === 'Escape') onCancel(); }}
>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label="Scegli i biglietti"
    tabindex="-1"
    bind:this={dialogEl}
    onclick={(e: MouseEvent) => e.stopPropagation()}
    onkeydown={onDialogKeydown}
  >
    <div class="dialog-head">
      <h2>Scegli i biglietti</h2>
      <p class="hint">
        Tieni almeno {minKeep} {minKeep > 1 ? 'biglietti' : 'biglietto'}
        &mdash; selezionati: {selected.size}
      </p>
    </div>

    <!-- Mappa con i soli biglietti selezionati: niente onRouteTap, solo pan/zoom. -->
    <div class="dialog-map">
      <MapSvg compact highlightTickets={pins} />
    </div>

    <div class="ticket-list">
      {#each offeredTickets as t, i (t.id)}
        {@const desc = ticketMap.get(t.id)}
        <button
          class="ticket-item"
          class:selected={selected.has(t.id)}
          aria-pressed={selected.has(t.id)}
          onclick={() => toggle(t.id)}
        >
          <TicketCard
            from={desc ? cityName(desc.from) : t.id}
            to={desc ? cityName(desc.to) : ''}
            points={t.points}
            long={desc?.deck === 'long'}
            selected={selected.has(t.id)}
            color={desc && selected.has(t.id) ? ticketPinColor(i) : undefined}
          />
        </button>
      {/each}
    </div>

    <div class="dialog-actions">
      <button class="btn cancel-btn" onclick={onCancel}> Annulla </button>
      <button class="btn confirm-btn" onclick={confirm} disabled={!canConfirm}>
        Conferma ({selected.size})
      </button>
    </div>
  </div>
</div>

<style>
  .dialog-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .dialog {
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    background: var(--chrome-bg-1);
    border: 2px solid var(--gold);
    border-radius: 14px;
    padding: 0.9rem;
    width: 96vw;
    max-width: 520px;
    height: 92vh;
    height: 92dvh;
    overflow: hidden;
    box-shadow: var(--shadow-board);
    color: var(--chrome-text);
  }
  /* Ritratto: mappa flessibile in alto, lista scorrevole e azioni sempre visibili sotto */
  .dialog-map {
    flex: 1 1 0;
    min-height: 140px;
    display: flex;
    flex-direction: column;
    margin-bottom: 0.75rem;
  }
  .dialog-map :global(.map-container) {
    flex: 1;
    height: auto;
    min-height: 0;
  }
  .dialog-map :global(.map-status) {
    flex: 1;
    min-height: 0;
  }
  h2 {
    color: var(--gold);
    margin: 0 0 0.5rem;
  }
  .hint {
    color: var(--chrome-muted);
    font-size: 0.85rem;
    margin: 0 0 1rem;
  }
  /* Biglietti su due colonne anche in ritratto: tutti visibili e più spazio alla mappa */
  .ticket-list {
    display: flex;
    flex-direction: row;
    flex-wrap: wrap;
    align-content: flex-start;
    gap: 0.6rem;
    flex: 0 1 auto;
    max-height: 45%;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    /* spazio per marcatore e anello di selezione, altrimenti tagliati dallo scroll */
    padding: 0.4rem;
  }
  .ticket-item {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    flex: 1 1 140px;
    width: auto;
    border-radius: 10px;
  }
  .ticket-item:focus-visible {
    outline: 2px solid var(--gold);
    outline-offset: 2px;
  }
  .dialog-actions {
    display: flex;
    gap: 0.75rem;
    margin-top: 0.75rem;
    flex: none;
  }
  .cancel-btn {
    background: #5a4a35;
    color: #fff;
    flex: 1;
  }
  .confirm-btn {
    background: #1e8449;
    color: #fff;
    flex: 1;
  }
  button {
    padding: 0.6rem;
    font-size: 1rem;
  }

  /* Landscape (telefono orizzontale): mappa a sinistra, titolo/biglietti/azioni a destra */
  @media (orientation: landscape) {
    .dialog {
      display: grid;
      grid-template-columns: minmax(0, 1.1fr) minmax(0, 1fr);
      grid-template-rows: auto minmax(0, 1fr) auto;
      column-gap: 0.9rem;
      max-width: 900px;
      height: 94vh;
      height: 94dvh;
      padding: 0.75rem 1rem;
    }
    .dialog-map {
      grid-column: 1;
      grid-row: 1 / span 3;
      margin-bottom: 0;
      min-height: 0;
    }
    .dialog-head {
      grid-column: 2;
      grid-row: 1;
    }
    .ticket-list {
      grid-column: 2;
      grid-row: 2;
      max-height: none;
    }
    .dialog-actions {
      grid-column: 2;
      grid-row: 3;
      margin-top: 0.6rem;
    }
    h2 {
      font-size: 1.1rem;
    }
    .hint {
      margin: 0 0 0.6rem;
    }
  }
</style>