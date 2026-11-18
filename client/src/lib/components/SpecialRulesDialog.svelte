<script lang="ts">
  // Modale «Regole speciali» del controller: si apre a inizio partita prima della scelta dei
  // biglietti e dal pulsante 📜. «Ho capito» (o Escape) lo chiude.

  import SpecialRules from './SpecialRules.svelte';

  let { title, rules, onClose }: {
    title: string | null;
    rules: readonly string[];
    onClose: () => void;
  } = $props();

  let okButton = $state<HTMLButtonElement>();

  // Focus sul pulsante e ripristino dell'elemento che lo aveva prima.
  $effect(() => {
    const previous = document.activeElement;
    okButton?.focus();
    return () => {
      if (previous instanceof HTMLElement) previous.focus();
    };
  });

  // Escape chiude; l'unico elemento focalizzabile è il pulsante, quindi Tab resta dentro.
  function onKeydown(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Escape') onClose();
    else if (e.key === 'Tab') {
      e.preventDefault();
      okButton?.focus();
    }
  }
</script>

<div class="rules-overlay" role="presentation">
  <div
    class="rules-dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="special-rules-title"
    tabindex="-1"
    onkeydown={onKeydown}
  >
    <div class="rules-body">
      <SpecialRules {title} {rules} />
    </div>
    <div class="rules-actions">
      <button class="btn ok-btn" bind:this={okButton} onclick={onClose}>Ho capito</button>
    </div>
  </div>
</div>

<style>
  .rules-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 110;
  }
  .rules-dialog {
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    background: var(--chrome-bg-1);
    border: 2px solid var(--gold);
    border-radius: 14px;
    padding: 0.8rem 1rem;
    width: 96vw;
    max-width: 640px;
    max-height: 92vh;
    max-height: 92dvh;
    box-shadow: var(--shadow-board);
    color: var(--chrome-text);
  }
  .rules-body {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .rules-actions {
    flex: none;
    margin-top: 0.7rem;
  }
  .ok-btn {
    width: 100%;
    padding: 0.6rem;
    font-size: 1rem;
    background: #1e8449;
    color: #fff;
  }
</style>
