<script lang="ts">
  // Simbolo del colore carta (per i daltonici), riempito con `currentColor`.
  // `halo` aggiunge un contorno chiaro per leggerlo su qualunque sfondo colorato.
  // Niente per la locomotiva.
  import type { CardColor } from '../proto';
  import { cardSymbolPath } from '../card-symbols';

  let { color, halo = false }: { color: CardColor; halo?: boolean } = $props();

  const d = $derived(cardSymbolPath(color));
</script>

{#if d}
  <svg class="card-symbol" viewBox="-2 -2 28 28" aria-hidden="true">
    <path {d} fill="currentColor"
      stroke={halo ? '#fdf7e6' : 'none'} stroke-width="3.2" stroke-linejoin="round" paint-order="stroke" />
  </svg>
{/if}

<style>
  .card-symbol {
    display: inline-block;
    width: 1em;
    height: 1em;
    vertical-align: -0.125em;
    flex: none;
  }
</style>
