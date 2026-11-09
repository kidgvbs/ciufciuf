<script lang="ts">
  // Carta treno disegnata da noi (Fase 3d): cornice pergamena, banda colore,
  // silhouette TrainIcon, nome colore (testo vero, a11y) e badge conteggio.
  //
  // Riutilizzata da: mano a ventaglio del controller, face-up (controller e
  // TV, con `badge` = numero posizione), pannello pagamento.
  import type { CardColor } from '../proto';
  import { CARD_COLOR_CSS } from '../colors';
  import TrainIcon from './TrainIcon.svelte';

  let {
    color,
    count = null,
    badge = null,
    small = false,
    handSize = null,
  }: {
    color: CardColor;
    /** Conteggio carte in mano (badge in alto a destra). */
    count?: number | null;
    /** Etichetta testuale in basso (es. posizione face-up sulla TV). */
    badge?: string | null;
    /** Variante compatta (es. pannello pagamento). */
    small?: boolean;
    /** Carte totali in mano: regola la larghezza nel ventaglio (landscape). */
    handSize?: number | null;
  } = $props();

  // Nel ventaglio le carte si restringono se la mano è piena, per restare
  // nella fascia senza scroll orizzontale (min 46px = target touch decente).
  const fanWidth = $derived(
    handSize !== null && handSize > 5
      ? Math.max(46, 64 - (handSize - 5) * 4)
      : null,
  );

  // Id unico per il gradient SVG della locomotiva (niente collisioni nel DOM).
  const uid = Math.random().toString(36).slice(2, 8);

  const NAMES: Record<CardColor, string> = {
    RED: 'Rossa',
    ORANGE: 'Arancio',
    YELLOW: 'Gialla',
    GREEN: 'Verde',
    BLUE: 'Blu',
    PURPLE: 'Viola',
    BLACK: 'Nera',
    WHITE: 'Bianca',
    LOCOMOTIVE: 'Jolly',
  };

  // Testo leggibile sulla banda colore (bianco sulle tonalità scure).
  const darkTextColors: CardColor[] = ['YELLOW', 'WHITE'];
  const bandText = $derived(darkTextColors.includes(color) ? '#453416' : '#fbf6e9');
</script>

<div class="train-card" class:small style="--band:{CARD_COLOR_CSS[color]}; --band-text:{bandText}{fanWidth !== null ? `; width:${fanWidth}px` : ''}">
  <div class="band"></div>
  <div class="art">
    <TrainIcon {color} {uid} />
  </div>
  <div class="card-name">{NAMES[color]}</div>
  {#if count !== null}
    <span class="count-badge" aria-label="{count} carte">{count}×</span>
  {/if}
  {#if badge !== null}
    <span class="pos-badge">{badge}</span>
  {/if}
</div>

<style>
  .train-card {
    position: relative;
    width: 64px;
    aspect-ratio: 2 / 3; /* proporzione carta treno §8.1 (~100×150) */
    background: var(--card-face);
    border-radius: var(--card-radius);
    border: 1px solid rgba(69, 52, 22, 0.55);
    box-shadow: var(--card-shadow);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    align-items: center;
    color: var(--board-ink);
  }
  .train-card.small {
    width: 44px;
  }
  .band {
    height: 16%;
    width: 100%;
    background: var(--band);
    border-bottom: 1px solid rgba(69, 52, 22, 0.4);
  }
  .art {
    flex: 1;
    width: 82%;
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 0;
  }
  .card-name {
    width: 100%;
    text-align: center;
    font-size: 0.55rem;
    font-weight: 700;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    color: var(--board-ink-soft);
    padding-bottom: 7%;
    line-height: 1;
  }
  .count-badge {
    position: absolute;
    top: 19%;
    right: 6%;
    background: var(--board-ink);
    color: var(--board-1);
    font-size: 0.6rem;
    font-weight: 800;
    line-height: 1;
    padding: 0.18rem 0.3rem;
    border-radius: 999px;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.4);
  }
  .pos-badge {
    position: absolute;
    bottom: 4%;
    right: 6%;
    /* N1 (review 3d): pill scura come il count-badge — contrasto ≥4.5:1
       su qualunque banda colore (prima: bg = colore banda, ORANGE/LOCO <4). */
    background: var(--board-ink);
    color: var(--board-1);
    font-size: 0.55rem;
    font-weight: 800;
    line-height: 1;
    min-width: 1.1em;
    text-align: center;
    padding: 0.18rem 0.25rem;
    border-radius: 999px;
    border: 1px solid rgba(0, 0, 0, 0.3);
  }
</style>
