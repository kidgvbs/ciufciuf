<script lang="ts">
  // Silhouette di treno originale (SVG inline, zero asset — regola d'oro 2).
  //
  // - color !== LOCOMOTIVE → carro merci (tramoggia + carrello ruote)
  // - color === LOCOMOTIVE → locomotiva con arcobaleno stilizzato (jolly)
  //
  // `color` tinge il corpo del mezzo; il carrello resta inchiostro scuro.
  import type { CardColor } from '../proto';
  import { CARD_COLOR_CSS } from '../colors';

  let { color, uid }: { color: CardColor; uid: string } = $props();

  const isLoco = $derived(color === 'LOCOMOTIVE');
  const bodyFill = $derived(isLoco ? '#3f3428' : CARD_COLOR_CSS[color]);
  const ink = '#332a1e';
  // Strisce arcobaleno originali (6 bande) per la locomotiva-jolly.
  const RAINBOW = ['#c0392b', '#d35400', '#eab308', '#1e8449', '#2471a3', '#7d3c98'];
</script>

<svg viewBox="0 0 48 22" width="100%" height="100%" aria-hidden="true">
  {#if isLoco}
    <defs>
      <linearGradient id="loco-rainbow-{uid}" x1="0" y1="0" x2="1" y2="0">
        {#each RAINBOW as c, i}
          <stop offset="{i / (RAINBOW.length - 1)}" stop-color={c} />
        {/each}
      </linearGradient>
    </defs>
    <!-- Arcobaleno sopra il camino -->
    <path d="M20 8 A 8 8 0 0 1 36 8" fill="none"
      stroke="url(#loco-rainbow-{uid})" stroke-width="3" stroke-linecap="round" />
    <!-- Camino + cupola -->
    <rect x="5.5" y="3.5" width="6" height="4" rx="1.4" fill={ink} />
    <circle cx="15" cy="9" r="2.4" fill={ink} />
    <!-- Cassa -->
    <rect x="2" y="8" width="41" height="7" rx="2.6" fill={bodyFill} stroke="#241d13" stroke-width="1" />
    <!-- Carrello -->
    <rect x="3" y="15" width="39" height="2.4" rx="1.2" fill={ink} />
    <circle cx="9" cy="18.5" r="2.7" fill={ink} stroke="#e9dcc0" stroke-width="1" />
    <circle cx="19" cy="18.5" r="2.7" fill={ink} stroke="#e9dcc0" stroke-width="1" />
    <circle cx="29" cy="18.5" r="2.7" fill={ink} stroke="#e9dcc0" stroke-width="1" />
    <circle cx="39" cy="18.5" r="2.7" fill={ink} stroke="#e9dcc0" stroke-width="1" />
  {:else}
    <!-- Tramoggia (punta sopra il cassone) -->
    <path d="M16 8.5 L24 2.5 L32 8.5 Z" fill={bodyFill} stroke="rgba(0,0,0,0.35)" stroke-width="0.8" />
    <!-- Cassone -->
    <rect x="4" y="8.5" width="40" height="7.5" rx="2.4" fill={bodyFill} stroke="rgba(0,0,0,0.35)" stroke-width="0.8" />
    <!-- Sponde del cassone -->
    <line x1="12" y1="9.5" x2="12" y2="15" stroke="rgba(0,0,0,0.28)" stroke-width="1" />
    <line x1="24" y1="9.5" x2="24" y2="15" stroke="rgba(0,0,0,0.28)" stroke-width="1" />
    <line x1="36" y1="9.5" x2="36" y2="15" stroke="rgba(0,0,0,0.28)" stroke-width="1" />
    <!-- Carrello -->
    <rect x="6" y="16" width="36" height="2" rx="1" fill={ink} />
    <circle cx="13" cy="19" r="2.5" fill={ink} stroke="#e9dcc0" stroke-width="1" />
    <circle cx="35" cy="19" r="2.5" fill={ink} stroke="#e9dcc0" stroke-width="1" />
  {/if}
</svg>
