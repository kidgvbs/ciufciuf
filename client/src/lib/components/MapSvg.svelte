<script lang="ts">
  // SVG mappa USA interattiva (pan & zoom).
  // Rende rotte (non reclamate + reclamate), città, vagoni.
  //
  // Layout SCHEMATICO precalcolato (deterministico) in usa-layout.json:
  // le posizioni delle città sono quelle del file generato, non una
  // proiezione delle coordinate geografiche.
  //
  // Props opzionali (mappa sul controller):
  // - highlightRouteIds: rotte da evidenziare con glow (es. claimabili da legalMoves)
  // - selectedRouteId:   rotta in pagamento (glow più marcato)
  // - onRouteTap:        tap/click su una rotta (distinto dal pan via soglia)
  // - compact:           stesso viewBox del full (layout) ma font/stroke/dot adattati
  //                      al telefono (stesso componente, niente fork)
  //
  // Revisione grafica:
  // - disegno a LAYER (ombre → letto binari → glow → tasselli/vagoni → aree
  //   tap → biglietti → città → etichette): uno sfondo disegnato dopo non
  //   copre più la linea di una rotta precedente, e nessun pallino copre
  //   un'etichetta altrui;
  // - tasselli separati (uno per carta) con bordo scuro: bianche/gialle
  //   leggibili sulla pergamena; vagoni reclamati "in rilievo" (ombra,
  //   riflesso, finestrini) distinguibili da una rotta dello stesso colore;
  // - offset doppie dipendente dallo spessore (doubleGap dal layout);
  // - font incluso (Barlow Semi Condensed 600) misurato dal generatore;
  // - pan/zoom in unità viewBox con pinch a due dita, limiti e pulsanti.
  import '@fontsource/barlow-semi-condensed/latin-600.css';
  import { publicStore } from '../stores.svelte.ts';
  import { PLAYER_COLOR_CSS } from '../colors';
  import * as MapUtils from '../map-utils';
  import type { MapData as MapDataType, Point, RouteData } from '../map-utils';
  import * as PZ from '../pan-zoom.ts';
  import usaMap from '../data/usa.json';
  import usaLayout from '../data/usa-layout.json';

  let {
    highlightRouteIds = [],
    selectedRouteId = null,
    onRouteTap,
    compact = false,
    highlightTickets = [],
  }: {
    highlightRouteIds?: string[];
    selectedRouteId?: string | null;
    onRouteTap?: (routeId: string) => void;
    compact?: boolean;
    /** Biglietti destinazione pinnati: pin + dashed line
        tra le due città, uno per biglietto, con colore ciascuno.
        Solo controller — la TV non li riceve mai. */
    highlightTickets?: Array<{ from: string; to: string }>;
  } = $props();

  const mapData = usaMap as MapDataType;
  const { routeIndex } = MapUtils.groupDoubleRoutes(mapData.routes);

  // Layout schematico generato (client/tools/gen-schematic-layout.mjs).
  interface UsaLayout {
    unit: number;
    width: number;
    height: number;
    cities: Record<string, Point>;
    labels: Record<string, { dx: number; dy: number; anchor: 'start' | 'middle' | 'end' }>;
    routes: Record<string, [number, number][]>;
    /** Font con cui il generatore ha validato le etichette. */
    font?: { family: string; weight: number; size: number };
    /** Gap libero tra i due binari di una doppia (px viewBox). */
    doubleGap?: number;
  }
  const layout = usaLayout as unknown as UsaLayout;
  const FONT_FAMILY = `'${layout.font?.family ?? 'Barlow Semi Condensed'}', system-ui, sans-serif`;
  const DOUBLE_GAP = layout.doubleGap ?? MapUtils.DEFAULT_DOUBLE_GAP;

  const interactive = $derived(onRouteTap !== undefined);
  const highlightSet = $derived(new Set(highlightRouteIds));

  // Nel compact gli stroke pensati per il full sembrano il doppio
  // sul telefono → fattore di scala solo visivo. Geometria invariata.
  const swFactor = $derived(compact ? 0.7 : 1);
  // Font etichette: full = quello validato dal generatore; compact 16
  // (il compact ha dot e stroke più piccoli → l'ingombro resta ≈ al full).
  const cityFontSize = $derived(compact ? 16 : (layout.font?.size ?? 15.5));
  const cityLabelStroke = $derived(compact ? 3.2 : 4.2);
  // TV (full) r=8; compact (controller) pallino più piccolo (6).
  const cityDotR = $derived(compact ? 6 : 8);
  const routeGap = $derived(cityDotR + 2.5);

  /** Stella a 5 punte (pin biglietto, disegno nostro) centrata in (cx,cy). */
  function starPath(cx: number, cy: number, rOut: number, rIn: number): string {
    let d = '';
    for (let i = 0; i < 10; i++) {
      const r = i % 2 === 0 ? rOut : rIn;
      const a = -Math.PI / 2 + (i * Math.PI) / 5;
      d += `${i === 0 ? 'M' : 'L'} ${(cx + r * Math.cos(a)).toFixed(2)} ${(cy + r * Math.sin(a)).toFixed(2)} `;
    }
    return d + 'Z';
  }

  // ── Geometria polyline (layout schematico) ──────────────────────────
  // Le rotte sono polyline a 2 o 3 punti (il 3° è il "bend" del layout).
  // offsetPoints sposta la polyline sulla normale locale (doppie);
  // trimPolyline accorcia le estremità lungo il PRIMO/ULTIMO segmento;
  // pointAlong interpola vagoni/slot sulla lunghezza totale.

  function routePoints(id: string): Point[] | null {
    const pts = layout.routes[id];
    return pts ? pts.map(([x, y]) => ({ x, y })) : null;
  }

  function offsetPoints(points: Point[], off: number): Point[] {
    return points.map((p, i) => {
      let segDx: number, segDy: number;
      if (i < points.length - 1) {
        segDx = points[i + 1]!.x - p.x;
        segDy = points[i + 1]!.y - p.y;
      } else {
        segDx = p.x - points[i - 1]!.x;
        segDy = p.y - points[i - 1]!.y;
      }
      const len = Math.hypot(segDx, segDy) || 1;
      return { x: p.x - (segDy / len) * off, y: p.y + (segDx / len) * off };
    });
  }

  function trimPolyline(points: Point[], trimStart: number, trimEnd: number): Point[] {
    const out = points.map(p => ({ ...p }));
    let rem = trimStart;
    while (rem > 0 && out.length > 1) {
      const a = out[0]!, b = out[1]!;
      const d = Math.hypot(b.x - a.x, b.y - a.y);
      if (d <= rem) { out.shift(); rem -= d; }
      else { out[0] = { x: a.x + ((b.x - a.x) / d) * rem, y: a.y + ((b.y - a.y) / d) * rem }; rem = 0; }
    }
    rem = trimEnd;
    while (rem > 0 && out.length > 1) {
      const a = out[out.length - 2]!, b = out[out.length - 1]!;
      const d = Math.hypot(b.x - a.x, b.y - a.y);
      if (d <= rem) { out.pop(); rem -= d; }
      else { out[out.length - 1] = { x: b.x - ((b.x - a.x) / d) * rem, y: b.y - ((b.y - a.y) / d) * rem }; rem = 0; }
    }
    return out;
  }

  // ── Quadratic Bézier helpers (curve lisce al bend) ────
  // Formula di de Casteljau: B(t) = (1-t)²P₀ + 2(1-t)tP₁ + t²P₂
  // La curva è sempre contenuta nel triangolo P₀-P₁-P₂.
  function bezierPoint(t: number, p0: Point, p1: Point, p2: Point): { x: number; y: number } {
    const u = 1 - t;
    return {
      x: u * u * p0.x + 2 * u * t * p1.x + t * t * p2.x,
      y: u * u * p0.y + 2 * u * t * p1.y + t * t * p2.y,
    };
  }

  function bezierTangent(t: number, p0: Point, p1: Point, p2: Point): { x: number; y: number } {
    const u = 1 - t;
    return {
      x: 2 * u * (p1.x - p0.x) + 2 * t * (p2.x - p1.x),
      y: 2 * u * (p1.y - p0.y) + 2 * t * (p2.y - p1.y),
    };
  }

  /** Arc-length parameterization: campiona la curva in 50 punti, cumula le
      lunghezze cordali, e binary-search per trovare il parametro t
      corrispondente a una data frazione (0..1) della lunghezza d'arco. */
  function pointAlongCurve(points: Point[], frac: number): { x: number; y: number; angle: number } {
    if (points.length === 2) return pointAlongLinear(points, frac);

    const [p0, p1, p2] = points as [Point, Point, Point];
    const N = 50;
    const table: { t: number; len: number }[] = [{ t: 0, len: 0 }];
    let prev = bezierPoint(0, p0, p1, p2);
    for (let i = 1; i <= N; i++) {
      const t = i / N;
      const pt = bezierPoint(t, p0, p1, p2);
      table[i] = { t, len: table[i - 1]!.len + Math.hypot(pt.x - prev.x, pt.y - prev.y) };
      prev = pt;
    }
    const total = table[N]!.len;
    const target = frac * total;

    let lo = 0, hi = N;
    while (lo < hi - 1) {
      const mid = (lo + hi) >> 1;
      if (table[mid]!.len < target) lo = mid; else hi = mid;
    }
    const segLen = table[hi]!.len - table[lo]!.len;
    const localFrac = segLen > 0 ? (target - table[lo]!.len) / segLen : 0;
    const t = table[lo]!.t + localFrac * (table[hi]!.t - table[lo]!.t);

    const pt = bezierPoint(t, p0, p1, p2);
    const tang = bezierTangent(t, p0, p1, p2);
    return { x: pt.x, y: pt.y, angle: Math.atan2(tang.y, tang.x) };
  }

  /** Lunghezza totale di una curva (2 punti → lineare, 3 punti → Q arc-length). */
  function curveLength(points: Point[]): number {
    if (points.length === 2) return polylineLength(points);
    const [p0, p1, p2] = points as [Point, Point, Point];
    const N = 50;
    let len = 0;
    let prev = bezierPoint(0, p0, p1, p2);
    for (let i = 1; i <= N; i++) {
      const pt = bezierPoint(i / N, p0, p1, p2);
      len += Math.hypot(pt.x - prev.x, pt.y - prev.y);
      prev = pt;
    }
    return len;
  }

  /** SVG path `d` per polyline/curva: 2 punti → L, 3 punti → quadratic Bézier. */
  function curveD(points: Point[]): string {
    const p = points.map(pt => pt.x + "," + pt.y);
    if (points.length === 3) return "M " + p[0] + " Q " + p[1] + " " + p[2];
    return "M " + p[0] + " L " + p[p.length - 1];
  }

  /** Trim lungo le tangenti: per la Q, inizio lungo P₀→C, fine lungo C→P₂. */
  function trimCurve(points: Point[], trimStart: number, trimEnd: number): Point[] {
    if (points.length === 2) return trimPolyline(points, trimStart, trimEnd);
    const [p0, p1, p2] = points as [Point, Point, Point];
    const tsx = p1.x - p0.x, tsy = p1.y - p0.y;
    const tsLen = Math.hypot(tsx, tsy) || 1;
    const tex = p2.x - p1.x, tey = p2.y - p1.y;
    const teLen = Math.hypot(tex, tey) || 1;
    return [
      { x: p0.x + (tsx / tsLen) * trimStart, y: p0.y + (tsy / tsLen) * trimStart },
      p1,
      { x: p2.x - (tex / teLen) * trimEnd, y: p2.y - (tey / teLen) * trimEnd },
    ];
  }

  // ── Leggasi lineari (fallback per rotte dritte) ────────────
  function pointAlongLinear(points: Point[], t: number): { x: number; y: number; angle: number } {
    const total = polylineLength(points);
    let target = t * total;
    for (let i = 0; i < points.length - 1; i++) {
      const a = points[i]!, b = points[i + 1]!;
      const segLen = Math.hypot(b.x - a.x, b.y - a.y);
      if (target <= segLen || i === points.length - 2) {
        const u = segLen > 0 ? target / segLen : 0;
        return {
          x: a.x + (b.x - a.x) * u,
          y: a.y + (b.y - a.y) * u,
          angle: Math.atan2(b.y - a.y, b.x - a.x),
        };
      }
      target -= segLen;
    }
    const last = points[points.length - 1]!;
    return { x: last.x, y: last.y, angle: 0 };
  }

  function polylineLength(points: Point[]): number {
    let total = 0;
    for (let i = 0; i < points.length - 1; i++) {
      const a = points[i]!, b = points[i + 1]!;
      total += Math.hypot(b.x - a.x, b.y - a.y);
    }
    return total;
  }

  // ── Geometria disegnata (precalcolata, indipendente da pan/zoom) ──────
  // Un tassello per carta: lungo quanto lo spazio disponibile meno un
  // piccolo gap (così si contano i vagoni), largo quanto lo stroke.
  interface Slot { x: number; y: number; deg: number }
  interface RouteGeo {
    route: RouteData;
    d: string;
    sw: number;
    slotL: number;
    slots: Slot[];
  }
  const routeGeos = $derived.by((): RouteGeo[] => {
    const out: RouteGeo[] = [];
    for (const route of mapData.routes) {
      const pts = routePoints(route.id);
      if (!pts) continue;
      // Offset doppie: metà spessore PIENO + metà gap (stessa formula del
      // generatore) — sul telefono lo stroke scala 0.7, il gap cresce.
      const off = route.double
        ? MapUtils.doubleRouteOffset(route.len, routeIndex.get(route.id) ?? 0, DOUBLE_GAP)
        : 0;
      const offPts = off === 0 ? pts : offsetPoints(pts, off);
      const sw = MapUtils.routeStrokeWidth(route.len) * swFactor;
      // Trim con cap: le rotte len-1 restano visibili (stesso cap del generatore).
      const trim = Math.min(routeGap + sw / 2, curveLength(pts) * 0.42);
      const trimmed = trimCurve(offPts, trim, trim);
      const len = curveLength(trimmed);
      const N = route.len;
      const slots: Slot[] = [];
      for (let i = 0; i < N; i++) {
        const pt = pointAlongCurve(trimmed, (i + 0.5) / N);
        slots.push({ x: pt.x, y: pt.y, deg: (pt.angle * 180) / Math.PI });
      }
      const slotL = Math.max(6, len / N - 3 * swFactor);
      out.push({ route, d: curveD(trimmed), sw, slotL, slots });
    }
    return out;
  });

  // Hub (≥6 collegamenti distinti): pallino "stazione" con anello doppio,
  // aiuta a riconoscere i nodi principali a colpo d'occhio.
  const cityDegree = (() => {
    const seen = new Set<string>();
    const deg = new Map<string, number>();
    for (const r of mapData.routes) {
      const k = [r.from, r.to].sort().join('|');
      if (seen.has(k)) continue;
      seen.add(k);
      deg.set(r.from, (deg.get(r.from) ?? 0) + 1);
      deg.set(r.to, (deg.get(r.to) ?? 0) + 1);
    }
    return deg;
  })();

  const cityPositions = new Map<string, Point>(Object.entries(layout.cities));

  // Etichette lontane dal pallino (vincolate dal generatore a ≤32px):
  // filetto sottile pallino→etichetta, così il nome resta legato alla sua
  // città anche nei cluster. Punto d'arrivo = bordo del testo verso il dot.
  const labelLeaders = $derived.by(() => {
    const fs = cityFontSize;
    const out: Array<{ id: string; x1: number; y1: number; x2: number; y2: number }> = [];
    for (const city of mapData.cities) {
      const pos = cityPositions.get(city.id);
      const l = layout.labels[city.id];
      if (!pos || !l) continue;
      // Ancora sul bordo del box testo più vicino al pallino
      let ex = l.dx;
      let ey = l.dy - 0.35 * fs; // centro verticale (cap height ≈ 0.7em)
      if (l.anchor === 'middle') ey = l.dy < 0 ? l.dy + 0.15 * fs : l.dy - 0.75 * fs;
      const dist = Math.hypot(ex, ey);
      if (dist - cityDotR < 14) continue; // vicina: nessun filetto
      const ux = ex / dist, uy = ey / dist;
      ex -= ux * 2; ey -= uy * 2;
      out.push({
        id: city.id,
        x1: pos.x + ux * (cityDotR + 1.5), y1: pos.y + uy * (cityDotR + 1.5),
        x2: pos.x + ex, y2: pos.y + ey,
      });
    }
    return out;
  });

  // ── Pan & Zoom (unità viewBox, pinch, limiti) ─────────────────────────
  // Canvas variabile: viewBox dal layout per ENTRAMBE le viste.
  const mapWidth = layout.width;
  const mapHeight = layout.height;
  const vb: PZ.Size = { width: mapWidth, height: mapHeight };
  let pz = $state<PZ.PanZoom>({ ...PZ.IDENTITY });
  let dragging = $state(false);
  let svgEl = $state<SVGSVGElement | null>(null);
  let containerEl = $state<HTMLDivElement | null>(null);
  const gesture = new PZ.GestureTracker({ tapSlopPx: 10 });
  // Rotta sotto il dito al pointerdown: il tap scatta al rilascio se il
  // gesto è rimasto un tap (niente click: il pointer capture sul container
  // lo dirotterebbe).
  let tapRouteId: string | null = null;

  function svgRect(): PZ.RectLike | null {
    return svgEl ? svgEl.getBoundingClientRect() : null;
  }

  function onDown(e: PointerEvent) {
    if (e.pointerType === 'mouse' && e.button !== 0) return;
    if (gesture.active === 0) {
      const el = (e.target as Element | null)?.closest?.('[data-route-id]');
      tapRouteId = el?.getAttribute('data-route-id') ?? null;
    }
    gesture.down(e.pointerId, e.clientX, e.clientY);
    containerEl?.setPointerCapture(e.pointerId);
    dragging = true;
  }
  function onMove(e: PointerEvent) {
    if (gesture.active === 0) return;
    const rect = svgRect();
    if (!rect) return;
    pz = gesture.move(e.pointerId, e.clientX, e.clientY, pz, rect, vb);
  }
  function onUp(e: PointerEvent) {
    if (gesture.active === 0) return;
    gesture.up(e.pointerId);
    if (gesture.active > 0) return;
    dragging = false;
    if (e.type === 'pointerup' && gesture.isTap && tapRouteId) {
      const id = tapRouteId;
      tapRouteId = null;
      if (interactive && !claimedMap.has(id)) onRouteTap?.(id);
    }
    tapRouteId = null;
  }
  function onWheel(e: WheelEvent) {
    e.preventDefault();
    const rect = svgRect();
    if (!rect) return;
    const focus = PZ.clientToViewBox(e.clientX, e.clientY, rect, vb);
    pz = PZ.zoomAt(pz, focus, PZ.wheelFactor(e.deltaY, e.deltaMode), vb);
  }
  function zoomButton(factor: number) {
    pz = PZ.zoomAt(pz, { x: mapWidth / 2, y: mapHeight / 2 }, factor, vb);
  }
  function resetView() {
    pz = { ...PZ.IDENTITY };
  }

  // wheel non passivo (preventDefault per non scrollare la pagina)
  $effect(() => {
    const el = containerEl;
    if (!el) return;
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  });

  const claimedMap = $derived(
    new Map(publicStore.claimedRoutes.map(cr => [cr.route_id, cr.claimed_by_player_id]))
  );

  // Layer multipli — un pin + una dashed line per OGNI
  // biglietto pinnato, colore per indice dalla palette (distingue i
  // tratteggiati quando più biglietti sono pinnati insieme). Decorativo:
  // pointer-events none, il tap-to-claim sulle rotte sottostanti resta
  // intatto. La sparizione al completamento la decide il controller
  // (filtra i biglietti `completed` segnalati dal server): qui zero logica.
  const TICKET_PIN_COLORS = ['#0f766e', '#b45309', '#7c3aed', '#a13d63', '#0369a1'];
  interface TicketLayer { from: Point; to: Point; d: string; color: string }
  const ticketLayers = $derived.by(() => {
    const layers: TicketLayer[] = [];
    highlightTickets.forEach((t, i) => {
      const fromPos = cityPositions.get(t.from);
      const toPos = cityPositions.get(t.to);
      if (!fromPos || !toPos) return;
      const dx = toPos.x - fromPos.x;
      const dy = toPos.y - fromPos.y;
      const dist = Math.hypot(dx, dy);
      if (dist === 0) return;
      // Curva leggera: controllo quadratico spostato sulla normale (~8% della distanza)
      const mx = (fromPos.x + toPos.x) / 2 - (dy / dist) * dist * 0.08;
      const my = (fromPos.y + toPos.y) / 2 + (dx / dist) * dist * 0.08;
      layers.push({
        from: fromPos,
        to: toPos,
        d: `M ${fromPos.x} ${fromPos.y} Q ${mx} ${my} ${toPos.x} ${toPos.y}`,
        color: TICKET_PIN_COLORS[i % TICKET_PIN_COLORS.length] ?? '#0f766e',
      });
    });
    return layers;
  });

  function playerColor(pid: string): string {
    const p = publicStore.playersSummary.find(ps => ps.player_id === pid);
    return p ? PLAYER_COLOR_CSS[p.color] : '#666';
  }

  // Tasselli liberi: il grigio "neutro" è più caldo e chiaro del vecchio
  // #95a5a6 (si legge meglio sulla pergamena); gli altri colori restano.
  function slotFill(route: RouteData): string {
    const c = MapUtils.routeStrokeColor(route);
    return c === '#95a5a6' ? '#b7ab95' : c;
  }

  // Ombra proiettata (luce da nord-ovest), in unità viewBox
  const SHADOW_DX = 1.6;
  const SHADOW_DY = 2.4;
</script>

<div
  class="map-container"
  class:compact
  class:dragging
  class:zoomed={pz.zoom > 1.001}
  role="application"
  aria-label="Mappa delle rotte"
  bind:this={containerEl}
  onpointerdown={onDown} onpointermove={onMove}
  onpointerup={onUp} onpointercancel={onUp} onlostpointercapture={onUp}
>
  <svg bind:this={svgEl} viewBox="0 0 {mapWidth} {mapHeight}" width="100%" height="100%">
    <defs>
      <!-- Riflesso/ombreggiatura dei tasselli: coordinate locali del
           tassello (ruotato), quindi la luce segue la direzione della rotta. -->
      <linearGradient id="slot-shade" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="#fff" stop-opacity="0.45" />
        <stop offset="0.45" stop-color="#fff" stop-opacity="0.06" />
        <stop offset="0.55" stop-color="#000" stop-opacity="0" />
        <stop offset="1" stop-color="#000" stop-opacity="0.28" />
      </linearGradient>
      <!-- Pallino città a "bottone" (luce da nord-ovest) -->
      <radialGradient id="city-dot" cx="0.38" cy="0.32" r="0.75">
        <stop offset="0" stop-color="#fffdf6" />
        <stop offset="0.6" stop-color="#f3e8cc" />
        <stop offset="1" stop-color="#cdb98f" />
      </radialGradient>
    </defs>

    <g transform="translate({pz.panX}, {pz.panY}) scale({pz.zoom})">
      <!-- 1. Ombre proiettate di tasselli e vagoni (rilievo) -->
      <g class="layer-shadow" aria-hidden="true">
        {#each routeGeos as g (g.route.id)}
          {@const claimed = claimedMap.has(g.route.id)}
          {@const w = claimed ? g.sw + 1.5 * swFactor : g.sw}
          {#each g.slots as s}
            <rect x={-g.slotL / 2} y={-w / 2} width={g.slotL} height={w} rx={w * 0.3}
              fill="#2a1d0c" opacity={claimed ? 0.32 : 0.18}
              transform="translate({s.x + SHADOW_DX * (claimed ? 1.4 : 1)} {s.y + SHADOW_DY * (claimed ? 1.4 : 1)}) rotate({s.deg})" />
          {/each}
        {/each}
      </g>

      <!-- 2. Letto dei binari: filo sottile continuo che lega i tasselli -->
      <g class="layer-bed" aria-hidden="true">
        {#each routeGeos as g (g.route.id)}
          <path d={g.d} fill="none" stroke="#6d5731" stroke-width={Math.max(2, g.sw * 0.28)}
            stroke-linecap="round" opacity="0.45" />
        {/each}
      </g>

      <!-- 3. Glow rotte evidenziate / in pagamento (sotto i tasselli) -->
      <g class="layer-glow" aria-hidden="true">
        {#each routeGeos as g (g.route.id)}
          {@const selected = g.route.id === selectedRouteId}
          {@const highlighted = !claimedMap.has(g.route.id) && highlightSet.has(g.route.id)}
          {#if highlighted || selected}
            <path d={g.d} fill="none"
              stroke={selected ? '#d35400' : '#e8a83e'}
              stroke-width={g.sw + (selected ? 12 : 9) * swFactor}
              stroke-linecap="round" stroke-linejoin="round"
              class="route-glow"
              style={selected ? 'animation:none;opacity:0.55' : ''} />
          {/if}
        {/each}
      </g>

      <!-- 4. Tasselli liberi / vagoni reclamati -->
      <g class="layer-slots">
        {#each routeGeos as g (g.route.id)}
          {@const claimedBy = claimedMap.get(g.route.id)}
          {#if claimedBy}
            {@const color = playerColor(claimedBy)}
            {@const w = g.sw + 1.5 * swFactor}
            {#each g.slots as s, i}
              <g transform="translate({s.x} {s.y}) rotate({s.deg})">
                <g class="wagon" style="animation-delay:{i * 70}ms">
                  <rect x={-g.slotL / 2} y={-w / 2} width={g.slotL} height={w} rx={w * 0.28}
                    fill={color} stroke="#1d150c" stroke-width={1.3 * swFactor} />
                  <rect x={-g.slotL / 2} y={-w / 2} width={g.slotL} height={w} rx={w * 0.28}
                    fill="url(#slot-shade)" />
                  <!-- Finestrini: distinguono il vagone da una rotta libera dello stesso colore -->
                  {#if g.slotL >= 14 * swFactor}
                    {@const winW = Math.min(5, g.slotL * 0.2)}
                    {@const winH = w * 0.32}
                    <rect x={-g.slotL / 4 - winW / 2} y={-w / 2 + w * 0.16} width={winW} height={winH} rx="1"
                      fill="#fdf7e6" opacity="0.85" />
                    <rect x={g.slotL / 4 - winW / 2} y={-w / 2 + w * 0.16} width={winW} height={winH} rx="1"
                      fill="#fdf7e6" opacity="0.85" />
                  {/if}
                </g>
              </g>
            {/each}
          {:else}
            {@const fill = slotFill(g.route)}
            {#each g.slots as s}
              <g transform="translate({s.x} {s.y}) rotate({s.deg})">
                <rect x={-g.slotL / 2} y={-g.sw / 2} width={g.slotL} height={g.sw} rx={g.sw * 0.22}
                  fill={fill} stroke="#4a3920" stroke-width={1.2 * swFactor} stroke-opacity="0.85" />
                <rect x={-g.slotL / 2} y={-g.sw / 2} width={g.slotL} height={g.sw} rx={g.sw * 0.22}
                  fill="url(#slot-shade)" opacity="0.8" />
              </g>
            {/each}
          {/if}
        {/each}
      </g>

      <!-- 5. Aree di tap generose (touch), invisibili -->
      {#if interactive}
        <g class="layer-hit">
          {#each routeGeos as g (g.route.id)}
            {#if !claimedMap.has(g.route.id)}
              <path d={g.d} data-route-id={g.route.id} class="route-tappable"
                fill="none" stroke="#000" stroke-opacity="0"
                stroke-width={g.sw + 14}
                stroke-linecap="round" stroke-linejoin="round"
                pointer-events="stroke" />
            {/if}
          {/each}
        </g>
      {/if}

      <!-- 6. Linee dei biglietti pinnati (sotto le città) -->
      {#each ticketLayers as layer}
        <path d={layer.d} class="ticket-line ticket-highlight" aria-hidden="true"
          fill="none" stroke={layer.color} stroke-width={3.5 * swFactor}
          stroke-linecap="round" stroke-dasharray="10 7" opacity="0.9" />
      {/each}

      <!-- 7. Città: ombra, pallino a bottone, anello doppio per gli hub -->
      <g class="layer-cities" aria-hidden="true">
        {#each labelLeaders as l (l.id)}
          <line x1={l.x1} y1={l.y1} x2={l.x2} y2={l.y2}
            stroke="#453416" stroke-width="1.1" stroke-opacity="0.55" stroke-linecap="round" />
        {/each}
        {#each mapData.cities as city (city.id)}
          {@const pos = cityPositions.get(city.id)}
          {#if pos}
            {@const hub = (cityDegree.get(city.id) ?? 0) >= 6}
            <circle cx={pos.x + SHADOW_DX} cy={pos.y + SHADOW_DY} r={cityDotR + (hub ? 1.5 : 0)}
              fill="#2a1d0c" opacity="0.25" />
            <circle cx={pos.x} cy={pos.y} r={cityDotR + (hub ? 1.5 : 0)} fill="url(#city-dot)"
              stroke="#453416" stroke-width="2" />
            {#if hub}
              <circle cx={pos.x} cy={pos.y} r={cityDotR * 0.45} fill="#453416" opacity="0.75" />
            {/if}
          {/if}
        {/each}
      </g>

      <!-- 8. Pin biglietti: anello attorno al pallino + stella a lato
           (non coprono più il pallino della città). -->
      {#each ticketLayers as layer}
        <g class="ticket-highlight" aria-hidden="true">
          {#each [layer.from, layer.to] as pinPos}
            <g transform="translate({pinPos.x} {pinPos.y})">
              <circle r={cityDotR + 4} fill="none" stroke={layer.color} stroke-width={2.5 * swFactor} />
              <circle r={cityDotR + 4} fill="none" stroke={layer.color} stroke-width={1.2 * swFactor}
                class="pin-pulse" />
              <g transform="translate({cityDotR + 5} {-(cityDotR + 5)})">
                <circle r={7.5 * swFactor + 1.5} fill="#fbf6e9" stroke={layer.color} stroke-width={1.5 * swFactor} />
                <path d={starPath(0, 0, 6 * swFactor, 2.6 * swFactor)} fill={layer.color} />
              </g>
            </g>
          {/each}
        </g>
      {/each}

      <!-- 9. Etichette: sempre in cima (nessun pallino/rotta le copre) -->
      <g class="layer-labels" aria-hidden="true">
        {#each mapData.cities as city (city.id)}
          {@const pos = cityPositions.get(city.id)}
          {@const label = layout.labels[city.id]}
          {#if pos && label}
            <text x={pos.x + label.dx} y={pos.y + label.dy}
              text-anchor={label.anchor}
              font-size={cityFontSize} font-weight="600"
              fill="#453416" stroke="#f6ebd0" stroke-width={cityLabelStroke}
              stroke-linejoin="round" paint-order="stroke"
              font-family={FONT_FAMILY}>{city.name}</text>
          {/if}
        {/each}
      </g>
    </g>
  </svg>

  <!-- Controlli zoom (il pinch resta il gesto principale sul telefono) -->
  <div class="zoom-controls" onpointerdown={(e) => e.stopPropagation()} role="group" aria-label="Zoom mappa">
    <button type="button" aria-label="Ingrandisci" onclick={() => zoomButton(1.5)} disabled={pz.zoom >= PZ.MAX_ZOOM}>+</button>
    <button type="button" aria-label="Riduci" onclick={() => zoomButton(1 / 1.5)} disabled={pz.zoom <= PZ.MIN_ZOOM}>−</button>
    {#if pz.zoom > 1.001}
      <button type="button" aria-label="Vista intera" onclick={resetView}>⤢</button>
    {/if}
  </div>
</div>

<style>
  /* Board in pergamena con cornice di legno: la mappa è una
     "scatola aperta sul tavolo" — bordo, ombra, angoli. */
  .map-container {
    flex: 1;
    position: relative;
    overflow: hidden;
    background:
      radial-gradient(circle at 30% 20%, rgba(255, 252, 240, 0.55), transparent 55%),
      url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='220' height='220'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='3' stitchTiles='stitch'/%3E%3CfeColorMatrix values='0 0 0 0 0.42 0 0 0 0 0.32 0 0 0 0 0.16 0 0 0 0.09 0'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E"),
      linear-gradient(155deg, var(--board-1), var(--board-2));
    /* Cornice di legno in rilievo: luce in alto a sinistra, ombra in basso a destra */
    border: 6px solid #6d5228;
    border-top-color: #8a6a36;
    border-left-color: #7d5f30;
    border-bottom-color: #4f3a1b;
    border-right-color: #5a4320;
    border-radius: 14px;
    margin: 12px;
    box-shadow: var(--shadow-board);
    touch-action: none;
    cursor: default;
    user-select: none;
    -webkit-user-select: none;
  }
  .map-container.zoomed { cursor: grab; }
  .map-container.zoomed.dragging { cursor: grabbing; }
  /* Vignettatura interna tipo carta invecchiata + bordo interno inciso */
  .map-container::after {
    content: '';
    position: absolute;
    inset: 0;
    pointer-events: none;
    border-radius: inherit;
    box-shadow:
      inset 0 0 70px rgba(96, 72, 34, 0.28),
      inset 0 2px 4px rgba(0, 0, 0, 0.25),
      inset 0 0 0 1px rgba(69, 52, 22, 0.5);
  }
  /* Variante compatta (telefono): il genitore dà l'altezza, la mappa riempie */
  .map-container.compact {
    width: auto;
    height: 100%;
    min-height: 220px;
    margin: 0;
    border-width: 3px;
    border-radius: 10px;
    box-shadow: var(--shadow-panel);
  }
  .map-container svg { display: block; }

  /* Prospettiva leggera solo sulla TV (sul telefono peggiorerebbe la lettura) */
  @media (min-width: 1200px) and (prefers-reduced-motion: no-preference) {
    .map-container:not(.compact) {
      transform: perspective(2200px) rotateX(4deg);
      transform-origin: 50% 100%;
    }
  }

  .zoom-controls {
    position: absolute;
    top: 8px;
    right: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    z-index: 1;
  }
  .zoom-controls button {
    width: 30px;
    height: 30px;
    padding: 0;
    border-radius: 8px;
    border: 1px solid rgba(69, 52, 22, 0.6);
    background: linear-gradient(#fbf3de, #e6d3a8);
    color: #453416;
    font-size: 18px;
    font-weight: 700;
    line-height: 1;
    box-shadow: 0 2px 3px rgba(0, 0, 0, 0.3);
    cursor: pointer;
  }
  .zoom-controls button:disabled { opacity: 0.4; cursor: default; }
  .compact .zoom-controls { top: 5px; right: 5px; }
  .compact .zoom-controls button { width: 28px; height: 28px; }

  .wagon {
    transform-box: fill-box;
    transform-origin: center;
    animation: wagon-in 0.35s ease-out both;
  }
  @keyframes wagon-in {
    from { opacity: 0; transform: translateY(-6px) scale(0.6); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }
  /* Glow sulle rotte evidenziate (claimabili / in pagamento): pulsante
     solo se l'utente non ha chiesto movimento ridotto. */
  .route-glow {
    opacity: 0.4;
    pointer-events: none;
  }
  @media (prefers-reduced-motion: no-preference) {
    .route-glow { animation: glow-pulse 1.2s ease-in-out infinite alternate; }
  }
  @keyframes glow-pulse {
    from { opacity: 0.18; }
    to { opacity: 0.5; }
  }
  .layer-shadow, .layer-bed, .layer-glow, .layer-slots, .layer-cities, .layer-labels {
    pointer-events: none;
  }
  .route-tappable {
    cursor: pointer;
    outline: none;
  }
  /* Biglietto evidenziato: intero layer non intercetta il puntatore */
  .ticket-highlight {
    pointer-events: none;
  }
  @media (prefers-reduced-motion: no-preference) {
    .ticket-line {
      animation: dash-march 1.6s linear infinite;
    }
    .pin-pulse {
      animation: pin-pulse 1.8s ease-out infinite;
      transform-box: fill-box;
      transform-origin: center;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .wagon { animation: none; }
  }
  @keyframes dash-march {
    to {
      stroke-dashoffset: -17;
    }
  }
  @keyframes pin-pulse {
    from {
      opacity: 0.7;
      transform: scale(1);
    }
    to {
      opacity: 0;
      transform: scale(1.6);
    }
  }
</style>
