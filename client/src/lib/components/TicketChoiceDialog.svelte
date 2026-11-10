<script lang="ts">
  // Dialog scelta biglietti (initial draft o pescata mid-game).

  import type { TicketInfo } from '../proto';
  import usaMap from '../data/usa.json';
  import type { MapData } from '../map-utils';
  import TicketCard from './TicketCard.svelte';

  const mapData = usaMap as MapData;
  const ticketMap = new Map(mapData.tickets.map((t) => [t.id, t]));
  const cityMap = new Map(mapData.cities.map((c) => [c.id, c]));

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
    const first = dialogEl?.querySelector<HTMLElement>(FOCUSABLE);
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
  onclick={onCancel}
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
    <h2>Scegli i biglietti</h2>
    <p class="hint">
      Tieni almeno {minKeep} biglietto{minKeep > 1 ? 'i' : ''}
      &mdash; selezionati: {selected.size}
    </p>

    <div class="ticket-list">
      {#each offeredTickets as t}
        {@const desc = ticketMap.get(t.id)}
        <button
          class="ticket-item"
          class:selected={selected.has(t.id)}
          onclick={() => toggle(t.id)}
        >
          <TicketCard
            from={desc ? cityName(desc.from) : t.id}
            to={desc ? cityName(desc.to) : ''}
            points={t.points}
            selected={selected.has(t.id)}
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
    background: var(--chrome-bg-1);
    border: 2px solid var(--gold);
    border-radius: 14px;
    padding: 1.5rem;
    width: 90%;
    max-width: 360px;
    max-height: 80vh;
    overflow-y: auto;
    box-shadow: var(--shadow-board);
    color: var(--chrome-text);
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
  .ticket-list {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .ticket-item {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    width: 100%;
    border-radius: 10px;
  }
  .ticket-item:focus-visible {
    outline: 2px solid var(--gold);
    outline-offset: 2px;
  }
  .dialog-actions {
    display: flex;
    gap: 0.75rem;
    margin-top: 1rem;
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

  /* Landscape (telefono orizzontale): larghezza contenuta ma biglietti
     affiancati — niente colonna fullscreen verticale */
  @media (orientation: landscape) {
    .dialog {
      max-width: 520px;
      max-height: 86vh;
      padding: 1rem 1.25rem;
    }
    .ticket-list {
      flex-direction: row;
      flex-wrap: wrap;
    }
    .ticket-item {
      flex: 1 1 200px;
      width: auto;
    }
    h2 {
      font-size: 1.1rem;
    }
    .hint {
      margin: 0 0 0.6rem;
    }
  }
</style>