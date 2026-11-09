<script lang="ts">
  // Card di selezione mappa stile board-game (lobby "Nuova partita").
  //
  // La thumbnail USA è un mini-render SVG originale dalle coordinate reali
  // (riusa la proiezione di map-utils) — zero asset ufficiali (regola d'oro 2).
  // Le mappe non disponibili mostrano un placeholder neutro disegnato da noi.
  import type { RulesetInfo } from '../rulesets';
  import * as MapUtils from '../map-utils';
  import type { MapData } from '../map-utils';
  import usaMap from '../data/usa.json';

  let { ruleset, selected, onSelect }: {
    ruleset: RulesetInfo;
    selected: boolean;
    onSelect: (id: string) => void;
  } = $props();

  const THUMB_W = 260;
  const THUMB_H = 170;

  // Mini-render della mappa USA dalle coordinate (stessa proiezione di MapSvg).
  const mapData = usaMap as MapData;
  const proj: MapUtils.ProjectionConfig = { width: THUMB_W, height: THUMB_H, padding: 14 };
  const cityPos = new Map(mapData.cities.map((c) => [c.id, MapUtils.projectCity(c, proj)]));
  const thumbRoutes = mapData.routes
    .map((r) => {
      const a = cityPos.get(r.from);
      const b = cityPos.get(r.to);
      return a && b ? { id: r.id, x1: a.x, y1: a.y, x2: b.x, y2: b.y } : null;
    })
    .filter((r): r is NonNullable<typeof r> => r !== null);
</script>

<button
  class="map-card"
  class:selected
  class:locked={!ruleset.available}
  disabled={!ruleset.available}
  onclick={() => onSelect(ruleset.id)}
  aria-pressed={selected}
>
  <div class="thumb">
    {#if ruleset.id === 'usa_v2'}
      <svg viewBox="0 0 {THUMB_W} {THUMB_H}" width="100%" height="100%" aria-hidden="true">
        <rect width={THUMB_W} height={THUMB_H} fill="#f0e3c2" rx="6" />
        {#each thumbRoutes as r}
          <line x1={r.x1} y1={r.y1} x2={r.x2} y2={r.y2}
            stroke="#a68b5b" stroke-width="1" stroke-linecap="round" opacity="0.6" />
        {/each}
        {#each mapData.cities as c}
          {@const p = cityPos.get(c.id)}
          {#if p}
            <circle cx={p.x} cy={p.y} r="1.8" fill="#57432a" opacity="0.9" />
          {/if}
        {/each}
      </svg>
    {:else}
      <!-- Placeholder neutro originale per mappe future (niente artwork esterno) -->
      <svg viewBox="0 0 {THUMB_W} {THUMB_H}" width="100%" height="100%" aria-hidden="true">
        <rect width={THUMB_W} height={THUMB_H} fill="#e8d9b8" rx="6" />
        <path d="M40 130 Q 90 60 140 100 T 225 55"
          fill="none" stroke="#a68b5b" stroke-width="3" stroke-linecap="round" stroke-dasharray="8 7" />
        <circle cx="40" cy="130" r="5" fill="#a68b5b" />
        <circle cx="140" cy="100" r="5" fill="#a68b5b" />
        <circle cx="225" cy="55" r="5" fill="#a68b5b" />
        <text x={THUMB_W / 2} y={THUMB_H / 2 + 36} text-anchor="middle"
          font-size="30" fill="#a68b5b" font-family="system-ui">?</text>
      </svg>
    {/if}
    {#if !ruleset.available}
      <span class="coming-soon">In arrivo</span>
    {/if}
  </div>
  <div class="card-name">{ruleset.name}</div>
  <div class="card-blurb">{ruleset.blurb}</div>
</button>

<style>
  .map-card {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    width: 100%;
    max-width: 240px;
    padding: 0.6rem;
    background: var(--chrome-bg-2);
    border: 3px solid var(--chrome-line);
    border-radius: 12px;
    color: var(--chrome-text);
    cursor: pointer;
    text-align: left;
    font-family: inherit;
    box-shadow: var(--shadow-panel);
    transition: transform 0.08s ease;
  }
  .map-card:hover:not(.locked) {
    border-color: var(--gold);
    transform: translateY(-2px);
  }
  .map-card.selected {
    border-color: var(--gold);
    box-shadow: 0 0 0 2px var(--gold), var(--shadow-panel);
  }
  .map-card.locked {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .thumb {
    position: relative;
    border-radius: 8px;
    overflow: hidden;
    border: 1px solid var(--board-edge);
  }
  .thumb svg {
    display: block;
  }
  .coming-soon {
    position: absolute;
    top: 6px;
    right: 6px;
    background: var(--gold);
    color: #453416;
    font-size: 0.7rem;
    font-weight: 700;
    padding: 0.15rem 0.5rem;
    border-radius: 10px;
  }
  .card-name {
    font-size: 1.05rem;
    font-weight: 700;
    color: var(--gold);
  }
  .card-blurb {
    font-size: 0.75rem;
    color: var(--chrome-muted);
  }
</style>
