<script lang="ts">
  // Una bandiera nazionale (disegni in `flags.ts`) centrata in (x, y), `width` × `height` unità
  // viewBox, con bordo opzionale. Va dentro un <svg>; il chiamante decide ruolo e ombra.
  import { FLAG_H, FLAG_W, flagShapes } from '../flags';

  let {
    code,
    x,
    y,
    width,
    height,
    border = 0,
    borderColor = '#453416',
  }: {
    code: string | undefined;
    x: number;
    y: number;
    width: number;
    height: number;
    /** Spessore del bordo (a cavallo del perimetro); 0 = nessun bordo. */
    border?: number;
    borderColor?: string;
  } = $props();

  const shapes = $derived(flagShapes(code) ?? []);
</script>

<g transform="translate({x - width / 2} {y - height / 2}) scale({width / FLAG_W} {height / FLAG_H})">
  {#each shapes as s}
    {#if s.kind === 'rect'}
      <rect x={s.x} y={s.y} width={s.w} height={s.h} fill={s.fill} />
    {:else}
      <path d={s.d} fill={s.fill} stroke={s.stroke} stroke-width={s.strokeWidth}
        stroke-linejoin="round" />
    {/if}
  {/each}
</g>
{#if border > 0}
  <rect x={x - width / 2} y={y - height / 2} {width} {height} fill="none"
    stroke={borderColor} stroke-width={border} stroke-linejoin="round" />
{/if}
