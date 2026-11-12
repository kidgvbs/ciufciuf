<script lang="ts">
  // Card di selezione mappa stile board-game (lobby "Nuova partita").
  //
  // La thumbnail è un mini-render SVG originale del layout schematico
  // (lo stesso che si vede in partita) — zero asset ufficiali (regola d'oro 2).
  // Finché mappa/layout non sono caricati (o non lo sono affatto) mostra un
  // placeholder neutro disegnato da noi. Il sottotitolo deriva dai dati del ruleset.
  import { rulesetBlurb, type RulesetInfo } from '../rulesets';
  import { loadRulesetMap, type RulesetMap } from '../map-loader';

  let { host, ruleset, selected, onSelect }: {
    host: string;
    ruleset: RulesetInfo;
    selected: boolean;
    onSelect: (id: string) => void;
  } = $props();

  const THUMB_W = 260;
  const THUMB_H = 170;

  // Mini-render del layout schematico (stesse posizioni e curve di MapSvg),
  // scalato in "meet" nella thumbnail con padding.
  // Mappa e layout arrivano dal server per il ruleset della card.
  let loaded = $state.raw<RulesetMap | null>(null);
  $effect(() => {
    loaded = null;
    let cancelled = false;
    loadRulesetMap(host, ruleset.id).then(
      (m) => { if (!cancelled) loaded = m; },
      (e: unknown) => { if (!cancelled) console.warn('[map-card] mappa non disponibile:', e); },
    );
    return () => { cancelled = true; };
  });
  const PAD = 12;
  const thumb = $derived.by(() => {
    const map = loaded?.map;
    const layout = loaded?.layout;
    if (!map || !layout) return null;
    const k = Math.min((THUMB_W - 2 * PAD) / layout.width, (THUMB_H - 2 * PAD) / layout.height);
    const ox = (THUMB_W - layout.width * k) / 2;
    const oy = (THUMB_H - layout.height * k) / 2;
    const tp = ([x, y]: [number, number]) => `${(ox + x * k).toFixed(1)},${(oy + y * k).toFixed(1)}`;
    const seenPairs = new Set<string>();
    const routes = map.routes
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
    const cities = map.cities.flatMap((c) => {
      const p = layout.cities[c.id];
      return p ? [{ x: ox + p.x * k, y: oy + p.y * k }] : [];
    });
    return { routes, cities };
  });
  const blurb = $derived(rulesetBlurb(ruleset));
</script>

<button
  class="map-card"
  class:selected
  onclick={() => onSelect(ruleset.id)}
  aria-pressed={selected}
>
  <div class="thumb">
    {#if thumb}
      <svg viewBox="0 0 {THUMB_W} {THUMB_H}" width="100%" height="100%" aria-hidden="true">
        <rect width={THUMB_W} height={THUMB_H} fill="#f0e3c2" rx="6" />
        {#each thumb.routes as r}
          <path d={r.d} fill="none"
            stroke="#a68b5b" stroke-width="1.4" stroke-linecap="round" opacity="0.75" />
        {/each}
        {#each thumb.cities as p}
          <circle cx={p.x} cy={p.y} r="1.8" fill="#57432a" opacity="0.9" />
        {/each}
      </svg>
    {:else}
      <!-- Placeholder neutro originale in attesa della mappa (niente artwork esterno) -->
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
  </div>
  <div class="card-name">{ruleset.name}</div>
  <div class="card-blurb">{blurb}</div>
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
  .map-card:hover {
    border-color: var(--gold);
    transform: translateY(-2px);
  }
  .map-card.selected {
    border-color: var(--gold);
    box-shadow: 0 0 0 2px var(--gold), var(--shadow-panel);
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
