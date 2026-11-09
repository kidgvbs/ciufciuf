<script lang="ts">
  // SVG mappa USA interattiva (pan & zoom).
  // Rende rotte (non reclamate + reclamate), città, vagoni.
  //
  // Fase 3f+5: layout SCHEMATICO precalcolato (deterministico) in
  // usa-layout.json — le posizioni città NON vengono più proiettate dalle
  // coordinate geografiche; il file generato è la verità a runtime.
  //
  // Props opzionali (Fase 3c, mappa sul controller):
  // - highlightRouteIds: rotte da evidenziare con glow (es. claimabili da legalMoves)
  // - selectedRouteId:   rotta in pagamento (glow più marcato)
  // - onRouteTap:        tap/click su una rotta (distinto dal pan via soglia)
  // - compact:           stesso viewBox del full (layout) ma font/stroke/dot adattati
  //                      al telefono (stesso componente, niente fork)
  import { publicStore } from '../stores.svelte.ts';
  import { PLAYER_COLOR_CSS } from '../colors';
  import * as MapUtils from '../map-utils';
  import type { MapData as MapDataType, Point } from '../map-utils';
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
    /** Biglietti destinazione pinnati (Fase 3f→3f+2): pin + dashed line
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
  }
  const layout = usaLayout as unknown as UsaLayout;

  const interactive = $derived(onRouteTap !== undefined);
  const highlightSet = $derived(new Set(highlightRouteIds));

  // Fase 3e: nel compact gli stroke pensati per il full sembrano il doppio
  // sul telefono → fattore di scala solo visivo. Geometria invariata.
  const swFactor = $derived(compact ? 0.7 : 1);
  // Nomi città: il canvas è ora quello del layout (~1097×609) per ENTRAMBE
  // le viste. Full 15.5 (validato dal generatore col caso peggiore: dot r=8,
  // stroke 1.0×). Compact 16: sul telefono il viewBox più largo rende le label
  // ~0.33–0.53× → più piccole; si compensa salendo a 16 (il compact ha dot
  // r=6 e stroke 0.7×, quindi più spazio, e l'ingombro resta ≈ al full).
  const cityFontSize = $derived(compact ? 16 : 15.5);
  const cityLabelStroke = $derived(compact ? 3.2 : 4.2);
  // 3f+4: TV (full) r=8; compact (controller) pallino più piccolo (6).
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
  // offsetPoints sposta la polyline sulla normale locale (doppie ±10);
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

// ── Quadratic Bézier helpers (3f+9: curve lisce al bend) ────
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


  // Canvas variabile: viewBox dal layout (usa-layout.json width/height) per
  // ENTRAMBE le viste (full e compact) — lo stesso disegno, il compact
  // differisce solo per swFactor/font/dot. Pan & zoom usano questi valori.
  const mapWidth = layout.width;
  const mapHeight = layout.height;
  let panX = $state(0);
  let panY = $state(0);
  let zoom = $state(1);
  let dragging = $state(false);
  let dragStart = $state({ x: 0, y: 0 });
  let dragPan = $state({ x: 0, y: 0 });
  // Tap vs pan: distanza percorsa durante il drag (non reattiva, solo handler).
  let movedBeyondTapSlop = false;

  const cityPositions = new Map<string, Point>(Object.entries(layout.cities));
  const claimedMap = $derived(
    new Map(publicStore.claimedRoutes.map(cr => [cr.route_id, cr.claimed_by_player_id]))
  );

  // Fase 3f→3f+2: layer multipli — un pin + una dashed line per OGNI
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

  // Pan & Zoom
  function onDown(e: PointerEvent) {
    dragging = true; dragStart = { x: e.clientX, y: e.clientY };
    dragPan = { x: panX, y: panY };
    movedBeyondTapSlop = false;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onMove(e: PointerEvent) {
    if (!dragging) return;
    panX = dragPan.x + (e.clientX - dragStart.x);
    panY = dragPan.y + (e.clientY - dragStart.y);
    if (Math.abs(e.clientX - dragStart.x) + Math.abs(e.clientY - dragStart.y) > 10) {
      movedBeyondTapSlop = true;
    }
  }
  function onUp() { dragging = false; }
  function onWheel(e: WheelEvent) {
    e.preventDefault();
    const delta = e.deltaY > 0 ? 0.9 : 1.1;
    const newZoom = Math.max(0.3, Math.min(4, zoom * delta));
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const mx = e.clientX - rect.left, my = e.clientY - rect.top;
    panX = mx - (mx - panX) * (newZoom / zoom);
    panY = my - (my - panY) * (newZoom / zoom);
    zoom = newZoom;
  }

  // Tap su rotta: scatta solo se il puntatore non ha pannato.
  function handleRouteTap(routeId: string) {
    if (movedBeyondTapSlop) { movedBeyondTapSlop = false; return; }
    onRouteTap?.(routeId);
  }

</script>

<div
  class="map-container"
  class:compact
  role="application"
  aria-label="Mappa delle rotte"
  onpointerdown={onDown} onpointermove={onMove}
  onpointerup={onUp} onpointerleave={onUp}
  onwheel={onWheel}
  style="cursor: {dragging ? 'grabbing' : 'grab'}"
>
  <svg viewBox="0 0 {mapWidth} {mapHeight}" width="100%" height="100%">
    <g transform="translate({panX}, {panY}) scale({zoom})">
      <!-- Rotte -->
      {#each mapData.routes as route}
        {@const pts = routePoints(route.id)}
        {#if pts}
          <!-- Offset doppie ±7 (perpendicolare al segmento locale); le
               singole restano centrate. 3f+3: estremi troncati prima del
               pallino lungo il PRIMO/ULTIMO segmento. -->
          {@const isDouble = route.double}
          {@const offMag = isDouble ? ((routeIndex.get(route.id) ?? 0) === 1 ? -7 : 7) : 0}
          {@const offsetPts = offMag === 0 ? pts : offsetPoints(pts, offMag)}
          {@const claimedBy = claimedMap.get(route.id)}
          {@const color = claimedBy ? playerColor(claimedBy) : MapUtils.routeStrokeColor(route)}
          {@const sw = MapUtils.routeStrokeWidth(route.len) * swFactor}
          <!-- Trim con cap: le rotte len-1 (26px) restano visibili (lo
               stesso cap del generatore, che lo usa anche per le etichette). -->
          {@const trim = Math.min(routeGap + sw / 2, curveLength(pts) * 0.42)}
          {@const trimmed = trimCurve(offsetPts, trim, trim)}
          {@const d = curveD(trimmed)}
          {@const N = route.len}
          {@const highlighted = !claimedBy && highlightSet.has(route.id)}
          {@const selected = route.id === selectedRouteId}

          <!-- Tap/click sulle rotte; il percorso keyboard-accessibile è la lista testuale (bottoni veri) -->
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <g
            class:route-tappable={interactive && !claimedBy}
            onclick={interactive ? () => handleRouteTap(route.id) : undefined}
          >
            <!-- Sfondo rotta -->
            <path d={d}
              fill="none"
              stroke={claimedBy ? color : '#8a7350'}
              stroke-width={sw + 4 * swFactor}
              stroke-linecap="round" stroke-linejoin="round"
              opacity={claimedBy ? 0.15 : 0.3} />

            <!-- Glow rotte evidenziate (es. claimabili da legalMoves) -->
            {#if highlighted || selected}
              <path d={d}
                fill="none"
                stroke={selected ? '#d35400' : '#e8a83e'}
                stroke-width={sw + (selected ? 12 : 9) * swFactor}
                stroke-linecap="round" stroke-linejoin="round"
                class="route-glow"
                style={selected ? 'animation:none;opacity:0.55' : ''} />
            {/if}

            <!-- Linea principale -->
            <path d={d}
              fill="none"
              stroke={color}
              stroke-width={sw}
              stroke-linecap="round" stroke-linejoin="round"
              opacity={claimedBy ? 0.9 : 0.7} />

            {#if interactive && !claimedBy}
              <!-- Area di tap generosa (touch) -->
              <path d={d}
                fill="none"
                stroke="#000" stroke-opacity="0"
                stroke-width={sw + 18}
                stroke-linecap="round" stroke-linejoin="round"
                pointer-events="stroke" />
            {/if}

            {#if claimedBy}
              <!-- Vagoni: comparsa sfalsata al claim (§8.2) -->
              {#each Array(N) as _, i}
                {@const pt = pointAlongCurve(trimmed, (i + 0.5) / N)}
                <circle cx={pt.x} cy={pt.y} r={sw * 0.4}
                  fill={color} stroke="#fff" stroke-width="1"
                  class="wagon" style="animation-delay:{i * 70}ms" />
              {/each}
            {:else}
              <!-- Slot vagoni: un tassello per lunghezza rotta -->
              {@const slotW = 14 * swFactor}
              {@const slotH = 9 * swFactor}
              {#each Array(N) as _, i}
                {@const pt = pointAlongCurve(trimmed, (i + 0.5) / N)}
                <rect
                  x={-slotW / 2} y={-slotH / 2}
                  width={slotW} height={slotH} rx={slotH / 2}
                  fill="#3a2b18" stroke={color} stroke-width="1.5" opacity="0.9"
                  transform="translate({pt.x} {pt.y}) rotate({(pt.angle * 180) / Math.PI})" />
              {/each}
            {/if}
          </g>
        {/if}
      {/each}

      <!-- Città (pallini + label) -->
      {#each mapData.cities as city}
        {@const pos = cityPositions.get(city.id)}
        {@const label = layout.labels[city.id]}
        {#if pos && label}
          <circle cx={pos.x} cy={pos.y} r={cityDotR} fill="#fbf6e9" stroke="#453416" stroke-width="2" />
          <text x={pos.x + label.dx} y={pos.y + label.dy}
            text-anchor={label.anchor}
            font-size={cityFontSize} font-weight="600"
            fill="#453416" stroke="#f6ebd0" stroke-width={cityLabelStroke} paint-order="stroke"
            font-family="system-ui">{city.name}</text>
        {/if}
      {/each}

      <!-- Biglietti evidenziati (Fase 3f→3f+2): pin + linea tratteggiata
           per ogni biglietto pinnato, ciascuno col suo colore. Solo
           decorazione: pointer-events none, il tap-to-claim sulle rotte
           sottostanti resta intatto. Mai sulla TV. -->
      {#each ticketLayers as layer}
        <g class="ticket-highlight" aria-hidden="true">
          <path d={layer.d} class="ticket-line"
            fill="none" stroke={layer.color} stroke-width={3.5 * swFactor}
            stroke-linecap="round" stroke-dasharray="10 7" opacity="0.9" />
          {#each [layer.from, layer.to] as pinPos}
            <g class="ticket-pin" transform="translate({pinPos.x} {pinPos.y})">
              <circle r={13 * swFactor} fill="#fbf6e9" stroke={layer.color} stroke-width={2.5 * swFactor} />
              <circle r={13 * swFactor} fill="none" stroke={layer.color} stroke-width={1.2 * swFactor}
                class="pin-pulse" />
              <path d={starPath(0, 0, 7.5 * swFactor, 3.2 * swFactor)} fill={layer.color} />
            </g>
          {/each}
        </g>
      {/each}
    </g>
  </svg>
</div>

<style>
  /* Board in pergamena con cornice di legno (Fase 3d): la mappa è una
     "scatola aperta sul tavolo" — bordo, ombra, angoli. Geometria invariata. */
  .map-container {
    flex: 1;
    position: relative;
    overflow: hidden;
    background:
      radial-gradient(circle at 30% 20%, rgba(255, 252, 240, 0.55), transparent 55%),
      linear-gradient(155deg, var(--board-1), var(--board-2));
    border: 6px solid #6d5228;
    border-radius: 14px;
    margin: 12px;
    box-shadow: var(--shadow-board);
    touch-action: none;
  }
  /* Vignettatura interna tipo carta invecchiata */
  .map-container::after {
    content: '';
    position: absolute;
    inset: 0;
    pointer-events: none;
    border-radius: inherit;
    box-shadow:
      inset 0 0 70px rgba(96, 72, 34, 0.28),
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
  .wagon {
    transform-box: fill-box;
    transform-origin: center;
    animation: wagon-in 0.35s ease-out both;
  }
  @keyframes wagon-in {
    from { opacity: 0; transform: scale(0.2); }
    to { opacity: 1; transform: scale(1); }
  }
  /* Glow pulsante sulle rotte evidenziate (claimabili / in pagamento) */
  .route-glow {
    animation: glow-pulse 1.2s ease-in-out infinite alternate;
    pointer-events: none;
  }
  @keyframes glow-pulse {
    from { opacity: 0.18; }
    to { opacity: 0.5; }
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
