<script lang="ts">
  // Biglietto destinazione in stile "tagliando ferroviario vintage" nostro
  // con città di partenza in alto, punti in grande al centro,
  // città di arrivo in basso. Spunta verde se completato.
  //
  // Usato nella mano del controller e nel dialog di scelta biglietti.
  let {
    from,
    to,
    points,
    completed = false,
    selected = false,
    active = false,
    onclick,
    color,
    long = false,
  }: {
    from: string;
    to: string;
    points: number;
    completed?: boolean;
    selected?: boolean;
    /** Evidenziato sulla mappa: pin + dashed line sulle due città. */
    active?: boolean;
    /** Se presente, il card è un <button> (toggle evidenza sulla mappa). */
    onclick?: () => void;
    /** Marcatore colorato: lo stesso colore della linea del biglietto sulla mappa. */
    color?: string;
    /** Biglietto lungo (mazzo `long`): badge "Lunga" e accento blu. */
    long?: boolean;
  } = $props();
</script>

<svelte:element
  this={onclick ? 'button' : 'div'}
  class="ticket"
  class:long
  class:completed
  class:selected
  class:active
  class:clickable={onclick !== undefined}
  type={onclick ? 'button' : undefined}
  role={onclick ? 'button' : undefined}
  {onclick}
>
  <div class="t-city from">{from}</div>
  <div class="t-mid">
    <span class="t-pts">{points}</span>
    <span class="t-pts-label">punti</span>
  </div>
  <div class="t-city to">{to}</div>
  {#if color}
    <span class="t-marker" style:background={color} aria-hidden="true"></span>
  {/if}
  {#if completed}
    <span class="t-check" aria-label="Biglietto completato">✓</span>
  {/if}
  {#if long}
    <span class="t-long">Lunga</span>
  {/if}
</svelte:element>

<style>
  .ticket {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: space-between;
    gap: 2px;
    padding: 8px 10px;
    background:
      radial-gradient(circle at 15% 20%, rgba(255, 255, 255, 0.5), transparent 45%),
      linear-gradient(160deg, var(--board-1), var(--board-2));
    border: 1.5px solid var(--board-edge);
    border-radius: 9px;
    box-shadow: var(--card-shadow);
    color: var(--board-ink);
    text-align: center;
    min-height: 74px;
    /* Quando è un <button>: eredità tipografica dal contesto */
    font-family: inherit;
  }
  /* Tratteggio interno stile tagliando */
  .ticket::before {
    content: '';
    position: absolute;
    inset: 4px;
    border: 1px dashed rgba(109, 87, 49, 0.5);
    border-radius: 6px;
    pointer-events: none;
  }
  .t-city {
    font-size: 0.72rem;
    font-weight: 700;
    line-height: 1.15;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .t-city.from {
    border-bottom: 1px solid rgba(109, 87, 49, 0.45);
    padding-bottom: 2px;
    width: 100%;
  }
  .t-mid {
    display: flex;
    align-items: baseline;
    gap: 4px;
  }
  .t-pts {
    font-size: 1.5rem;
    font-weight: 800;
    color: var(--gold-deep);
    line-height: 1;
  }
  .t-pts-label {
    font-size: 0.55rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--board-ink-soft);
  }
  .ticket.long {
    border-color: #2b5f8a;
    background:
      radial-gradient(circle at 15% 20%, rgba(255, 255, 255, 0.5), transparent 45%),
      linear-gradient(160deg, #dbe8f1, #b9d0e0);
  }
  .ticket.long::before {
    border-color: rgba(43, 95, 138, 0.5);
  }
  .t-long {
    position: absolute;
    bottom: -6px;
    right: 6px;
    padding: 0 6px;
    border-radius: 8px;
    background: #2b5f8a;
    color: #f4f8fb;
    font-size: 0.55rem;
    font-weight: 800;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    line-height: 1.5;
    border: 1px solid var(--board-1);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
  }
  .ticket.completed {
    border-color: #1e8449;
  }
  .ticket.completed::before {
    border-color: rgba(30, 132, 73, 0.55);
  }
  .ticket.completed .t-pts {
    color: #1e8449;
  }
  .ticket.selected {
    border-color: var(--gold);
    box-shadow:
      0 0 0 2px var(--gold),
      var(--card-shadow);
  }
  /* Attivo = pinnato sulla mappa: bordo teal come i pin/linea */
  .ticket.active {
    border-color: #0f766e;
    box-shadow:
      0 0 0 2px #0f766e,
      var(--card-shadow);
  }
  .ticket.clickable {
    cursor: pointer;
  }
  .ticket.clickable:focus-visible {
    outline: 2px solid #0f766e;
    outline-offset: 2px;
  }
  .t-marker {
    position: absolute;
    top: -5px;
    left: -5px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 2px solid var(--board-1);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
  }
  .t-check {
    position: absolute;
    top: -7px;
    right: -7px;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: #1e8449;
    color: #fbf6e9;
    font-size: 0.8rem;
    font-weight: 800;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 2px solid var(--board-1);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
  }
</style>
