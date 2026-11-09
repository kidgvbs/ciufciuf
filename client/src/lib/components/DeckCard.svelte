<script lang="ts">
  // Pila del mazzo coperto (Fase 3e): retro di carta disegnato da noi —
  // fondo inchiostro con righe diagonali dorate, medaglione pergamena con
  // silhouette TrainIcon. CSS/SVG 100% originale (regola d'oro 2).
  //
  // Usato sulla TV accanto alle carte scoperte; il badge mostra le carte
  // rimaste nel mazzo (`PublicState.deck_size` dal server — dato, non logica).
  import TrainIcon from './TrainIcon.svelte';

  let { count, small = false }: { count: number; small?: boolean } = $props();

  // Id unico per eventuali gradient SVG interni a TrainIcon.
  const uid = Math.random().toString(36).slice(2, 8);
</script>

<div class="deck-card" class:small role="img" aria-label="Mazzo: {count} carte">
  <div class="medallion" aria-hidden="true">
    <TrainIcon color="BLACK" {uid} />
  </div>
  <span class="deck-count" aria-hidden="true">{count}</span>
</div>

<style>
  .deck-card {
    position: relative;
    width: 64px;
    aspect-ratio: 2 / 3; /* stessa proporzione delle TrainCard */
    border-radius: var(--card-radius);
    border: 1px solid rgba(232, 168, 62, 0.55);
    background:
      repeating-linear-gradient(
        45deg,
        rgba(232, 168, 62, 0.14) 0 5px,
        transparent 5px 11px
      ),
      linear-gradient(160deg, #3a2b18, #241a0e);
    /* Pila di carte: due spessori sfalsati sotto il dorso */
    box-shadow:
      2px 2px 0 #efe3c4,
      4px 4px 0 rgba(109, 87, 49, 0.85),
      5px 6px 8px rgba(23, 16, 8, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .deck-card.small {
    width: 44px;
  }
  .medallion {
    width: 68%;
    aspect-ratio: 1;
    border-radius: 50%;
    background: var(--card-face);
    border: 2px solid var(--gold-deep);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 10%;
    box-sizing: border-box;
  }
  .deck-count {
    position: absolute;
    bottom: -8px;
    right: -10px;
    min-width: 1.6em;
    text-align: center;
    background: var(--gold);
    color: #453416; /* ≥7:1 su gold, come turn-tag */
    font-weight: 800;
    font-size: 1rem;
    line-height: 1;
    padding: 0.3rem 0.5rem;
    border-radius: 999px;
    border: 2px solid var(--chrome-bg-0);
    box-shadow: 0 2px 5px rgba(0, 0, 0, 0.55);
  }
  .deck-card.small .deck-count {
    font-size: 0.8rem;
    bottom: -6px;
    right: -8px;
  }
</style>
