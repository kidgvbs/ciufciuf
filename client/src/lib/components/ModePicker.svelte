<script lang="ts">
  // Selettore delle modalità di gioco di una mappa («Nuova partita»): una scelta singola fra i
  // ruleset della stessa mappa, ciascuno con modalità e sottotitolo derivato dai dati del server.
  import { modeBlurb, type RulesetInfo } from '../rulesets';

  let { modes, value, onSelect }: {
    /** Ruleset della mappa, già ordinati per `modeOrder`. */
    modes: RulesetInfo[];
    /** Id del ruleset selezionato. */
    value: string;
    onSelect: (id: string) => void;
  } = $props();

  let buttons = $state<HTMLButtonElement[]>([]);

  // Radiogroup da tastiera: frecce spostano selezione e focus (con wrap), Home/End vanno agli estremi.
  function onKeydown(e: KeyboardEvent, index: number) {
    let next: number;
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') next = (index + 1) % modes.length;
    else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') next = (index - 1 + modes.length) % modes.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = modes.length - 1;
    else return;
    e.preventDefault();
    onSelect(modes[next]!.id);
    buttons[next]?.focus();
  }

  // Un solo gruppo di tabulazione: la modalità selezionata (o la prima se la selezione non è qui).
  const tabStop = $derived(Math.max(0, modes.findIndex((m) => m.id === value)));
</script>

{#if modes.length > 1}
  <div class="mode-picker" role="radiogroup" aria-label="Modalità di gioco">
    {#each modes as m, i (m.id)}
      <button
        type="button"
        class="mode"
        class:selected={m.id === value}
        role="radio"
        aria-checked={m.id === value}
        tabindex={i === tabStop ? 0 : -1}
        bind:this={buttons[i]}
        onclick={() => onSelect(m.id)}
        onkeydown={(e) => onKeydown(e, i)}
      >
        <span class="mode-title">{m.mode}</span>
        <span class="mode-blurb">{modeBlurb(m)}</span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .mode-picker {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin: 0.5rem 0 0.75rem;
  }
  .mode {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    max-width: 16rem;
    margin: 0;
    padding: 0.45rem 0.75rem;
    background: var(--chrome-bg-2);
    border: 2px solid var(--chrome-line);
    border-radius: 10px;
    color: var(--chrome-text);
    cursor: pointer;
    text-align: left;
    font-family: inherit;
  }
  .mode:hover,
  .mode:focus-visible {
    border-color: var(--gold);
  }
  .mode.selected {
    border-color: var(--gold);
    box-shadow: 0 0 0 2px var(--gold);
  }
  .mode-title {
    font-size: 0.95rem;
    font-weight: 700;
    color: var(--gold);
  }
  .mode-blurb {
    font-size: 0.72rem;
    color: var(--chrome-muted);
  }
</style>
