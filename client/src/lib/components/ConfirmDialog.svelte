<script lang="ts">
  // Dialog di conferma modale (sostituisce `window.confirm`).

  import { createOverlayDismiss } from '../overlay-dismiss';

  let { title, message, confirmLabel = 'Conferma', cancelLabel = 'Annulla', onConfirm, onCancel }: {
    title: string;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  const overlay = createOverlayDismiss(() => onCancel());

  const FOCUSABLE = 'button:not([disabled])';

  // Focus iniziale sul bottone di annullamento e ripristino dell'elemento precedente.
  let dialogEl = $state<HTMLDivElement>();
  $effect(() => {
    const previous = document.activeElement;
    const first = dialogEl?.querySelector<HTMLElement>(FOCUSABLE);
    (first ?? dialogEl)?.focus();

    // Se il focus finisce su `body` (clic su un'area non focalizzabile) torna nel dialog.
    const reclaimFocus = () => {
      if (!dialogEl || (document.activeElement && document.activeElement !== document.body)) return;
      (dialogEl.querySelector<HTMLElement>(FOCUSABLE) ?? dialogEl).focus();
    };
    let timer: ReturnType<typeof setTimeout> | undefined;
    const onFocusOut = () => {
      clearTimeout(timer);
      timer = setTimeout(reclaimFocus, 0);
    };
    // Con il focus su `body` Escape e Tab non raggiungono il dialog: li gestisce questo listener.
    // Gli altri tasti (Invio, Spazio) non devono attivare un bottone al posto dell'utente.
    const onDocumentKeydown = (e: KeyboardEvent) => {
      if (document.activeElement && document.activeElement !== document.body) return;
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        onCancel();
      } else if (e.key === 'Tab') {
        e.preventDefault();
        reclaimFocus();
      }
    };
    document.addEventListener('focusout', onFocusOut);
    document.addEventListener('keydown', onDocumentKeydown, true);
    return () => {
      clearTimeout(timer);
      document.removeEventListener('focusout', onFocusOut);
      document.removeEventListener('keydown', onDocumentKeydown, true);
      if (previous instanceof HTMLElement) previous.focus();
    };
  });

  // Escape annulla; Tab e Maiusc+Tab restano dentro il dialog.
  function onKeydown(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Escape') {
      onCancel();
      return;
    }
    if (e.key !== 'Tab' || !dialogEl) return;
    const items = [...dialogEl.querySelectorAll<HTMLElement>(FOCUSABLE)];
    const first = items[0];
    const last = items[items.length - 1];
    if (!first || !last) return;
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
  class="overlay"
  role="presentation"
  onpointerdown={overlay.onpointerdown}
  onpointerup={overlay.onpointerup}
  onclick={overlay.onclick}
>
  <div
    class="dialog"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="confirm-title"
    aria-describedby="confirm-message"
    tabindex="-1"
    bind:this={dialogEl}
    onclick={(e: MouseEvent) => e.stopPropagation()}
    onkeydown={onKeydown}
  >
    <h2 id="confirm-title">{title}</h2>
    <p id="confirm-message">{message}</p>
    <div class="actions">
      <button class="btn cancel-btn" onclick={onCancel}>{cancelLabel}</button>
      <button class="btn confirm-btn" onclick={onConfirm}>{confirmLabel}</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 110;
  }
  .dialog {
    background: var(--chrome-bg-1);
    border: 2px solid var(--gold);
    border-radius: 14px;
    padding: 1.5rem;
    width: 90%;
    max-width: 360px;
    box-shadow: var(--shadow-board);
    color: var(--chrome-text);
  }
  h2 {
    color: var(--gold);
    margin: 0 0 0.5rem;
    font-size: 1.2rem;
  }
  p {
    margin: 0 0 1rem;
    color: var(--chrome-muted);
  }
  .actions {
    display: flex;
    gap: 0.75rem;
  }
  button {
    flex: 1;
    min-height: 44px;
    padding: 0.6rem;
    font-size: 1rem;
    color: #fff;
  }
  .cancel-btn {
    background: #5a4a35;
  }
  .confirm-btn {
    background: #a93226;
  }
</style>
