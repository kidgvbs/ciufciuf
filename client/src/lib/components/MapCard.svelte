<script lang="ts">
  // Card di selezione mappa stile board-game (lobby "Nuova partita").
  //
  // La thumbnail USA è un mini-render SVG originale del layout schematico
  // (lo stesso che si vede in partita) — zero asset ufficiali (regola d'oro 2).
  // Le mappe non disponibili mostrano un placeholder neutro disegnato da noi.
  import type { RulesetInfo } from '../rulesets';
  import type { MapData } from '../map-utils';
  import usaMap from '../data/usa.json';
  import usaLayout from '../data/usa-layout.json';

  let { ruleset, selected, onSelect }: {
    ruleset: RulesetInfo;
    selected: boolean;
    onSelect: (id: string) => void;
  } = $props();

  const THUMB_W = 260;
  const THUMB_H = 170;

  // Mini-render del layout schematico (stesse posizioni e curve di MapSvg),
  // scalato in "meet" nella thumbnail con padding.
  const mapData = usaMap as MapData;
  const layout = usaLayout as unknown as {
    width: number;
    height: number;
    cities: Record<string, { x: number; y: number }>;
    routes: Record<string, [number, number][]>;
  };
  const PAD = 12;
  const k = Math.min((THUMB_W - 2 * PAD) / layout.width, (THUMB_H - 2 * PAD) / layout.height);
  const ox = (THUMB_W - layout.width * k) / 2;
  const oy = (THUMB_H - layout.height * k) / 2;
  const tp = ([x, y]: [number, number]) => `${(ox + x * k).toFixed(1)},${(oy + y * k).toFixed(1)}`;
  const cityPos = new Map(
    Object.entries(layout.cities).map(([id, p]) => [id, { x: ox + p.x * k, y: oy + p.y * k }]),
  );
  const seenPairs = new Set<string>();
  const thumbRoutes = mapData.routes
    .filter((r) => {
      const key = [r.from, r.to].sort().join('|');
      if (seenPairs.has(key)) return false;
      seenPairs.add(key);
      return true;
    })
    .map((r) => {
      const pts = layout.routes[r.id];
      if (!pts || pts.length < 2) return null;
      const [a, b, c] = pts;
      const d = c ? `M ${tp(a!)} Q ${tp(b!)} ${tp(c)}` : `M ${tp(a!)} L ${tp(b!)}`;
      return { id: r.id, d };
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
          <path d={r.d} fill="none"
            stroke="#a68b5b" stroke-width="1.4" stroke-linecap="round" opacity="0.75" />
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
