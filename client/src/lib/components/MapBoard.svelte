<script lang="ts">
  // SVG mappa interattiva (pan & zoom); dati di mappa dal prop `mapData`, layout schematico dal prop `layout`.
  // Rende rotte (non reclamate + reclamate), città, vagoni.
  //
  // Layout SCHEMATICO precalcolato (deterministico) dal prop `layout`
  // (data/layouts/{mapId}.json, servito dal server): le posizioni delle città
  // sono quelle del file generato, non una proiezione delle coordinate geografiche.
  //
  // Props opzionali (mappa sul controller):
  // - highlightRouteIds: rotte da evidenziare con glow (es. claimabili da legalMoves)
  // - selectedRouteId:   rotta in pagamento (glow più marcato)
  // - onRouteTap:        tap/click su una rotta (distinto dal pan via soglia)
  // - compact:           stesso viewBox del full (layout) ma font/stroke/dot adattati
  //                      al telefono (stesso componente, niente fork)
  // - focusClaim:        solo TV: zoom animato sulla rotta appena occupata (`{seq, routeId}`;
  //                      ogni `seq` nuovo è un nuovo evento), poi ritorno alla vista precedente.
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
  import { untrack } from 'svelte';
  import { publicStore } from '../stores.svelte.ts';
  import { PLAYER_COLOR_CSS } from '../colors';
  import { passengerColorCss } from '../passengers';
  import * as MapUtils from '../map-utils';
  import type { MapData as MapDataType, MapLayout, Point, RouteData } from '../map-utils';
  import * as PZ from '../pan-zoom.ts';

  let {
    mapData,
    layout,
    highlightRouteIds = [],
    selectedRouteId = null,
    onRouteTap,
    compact = false,
    highlightTickets = [],
    selectableCityIds = [],
    selectedCityId = null,
    onCityTap,
    bigCities = [],
    focusClaim = null,
  }: {
    /** Mappa già caricata; il wrapper `MapSvg` rimonta il componente se cambia. */
    mapData: MapDataType;
    /** Layout schematico della stessa mappa (già verificato da `map-loader`). */
    layout: MapLayout;
    highlightRouteIds?: string[];
    selectedRouteId?: string | null;
    onRouteTap?: (routeId: string) => void;
    compact?: boolean;
    /** Biglietti destinazione pinnati: pin sull'origine e su ogni destinazione + dashed line
        dall'origine a ciascuna, uno per biglietto, con colore ciascuno.
        Solo controller — la TV non li riceve mai. */
    highlightTickets?: Array<{ from: string; to: string[] }>;
    /** Europe: città selezionabili (es. dove costruire una stazione): anello pulsante + area di tap. */
    selectableCityIds?: string[];
    /** Città scelta (anello pieno). */
    selectedCityId?: string | null;
    /** Tap/click su una città selezionabile. */
    onCityTap?: (cityId: string) => void;
    /** Modalità «Grandi Città»: città marcate con un anello (solo visualizzazione, non interattive). */
    bigCities?: string[];
    /** TV: tratta appena occupata su cui zoomare (non mostra nulla se `compact`). */
    focusClaim?: { seq: number; routeId: string } | null;
  } = $props();
  // Id unici per istanza: nel dialog biglietti due mappe coesistono (quella di sfondo
  // può essere `display:none`, e i gradienti lì dentro non vengono risolti).
  const uid = $props.id();

  const routeGroups = $derived(MapUtils.groupDoubleRoutes(mapData.routes));

  const FONT_FAMILY = $derived(
    `'${layout.font?.family ?? 'Barlow Semi Condensed'}', system-ui, sans-serif`,
  );
  const DOUBLE_GAP = $derived(layout.doubleGap ?? MapUtils.DEFAULT_DOUBLE_GAP);

  const interactive = $derived(onRouteTap !== undefined);
  const highlightSet = $derived(new Set(highlightRouteIds));
  const selectableCitySet = $derived(new Set(selectableCityIds));
  // Binari liberi che nessuno può più reclamare (gruppo parallelo già preso con pochi giocatori):
  // non si disegnano. Il binario preso resta dov'è (offset invariato): centrarlo farebbe saltare i
  // vagoni già posati.
  const blockedSet = $derived(new Set(publicStore.blockedRouteIds));
  const bigCitySet = $derived(new Set(bigCities));
  const cityLabel = (city: { id: string; name: string }) =>
    bigCitySet.has(city.id) ? `${city.name}, grande città` : city.name;
  const cityInteractive = $derived(onCityTap !== undefined && selectableCityIds.length > 0);

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
      if (blockedSet.has(route.id) && !claimedMap.has(route.id)) continue;
      const pts = routePoints(route.id);
      if (!pts) continue;
      // Offset dei gruppi paralleli (2–4 binari, simmetrici attorno alla linea): passo =
      // spessore PIENO + gap (stessa formula del generatore) — sul telefono lo stroke
      // scala 0.7, il gap cresce.
      const lanes = routeGroups.routeGroupSize.get(route.id) ?? 1;
      const off = route.double && lanes > 1
        ? MapUtils.doubleRouteOffset(route.len, routeGroups.routeIndex.get(route.id) ?? 0, DOUBLE_GAP, lanes)
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

  const cityPositions = $derived(new Map<string, Point>(Object.entries(layout.cities)));

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
  const mapWidth = $derived(layout.width);
  const mapHeight = $derived(layout.height);
  const vb: PZ.Size = $derived({ width: mapWidth, height: mapHeight });
  let pz = $state<PZ.PanZoom>({ ...PZ.IDENTITY });
  let dragging = $state(false);
  let svgEl = $state<SVGSVGElement | null>(null);
  let containerEl = $state<HTMLDivElement | null>(null);
  const gesture = new PZ.GestureTracker({ tapSlopPx: 10 });
  // Rotta sotto il dito al pointerdown: il tap scatta al rilascio se il
  // gesto è rimasto un tap (niente click: il pointer capture sul container
  // lo dirotterebbe).
  let tapRouteId: string | null = null;
  let tapCityId: string | null = null;

  function svgRect(): PZ.RectLike | null {
    return svgEl ? svgEl.getBoundingClientRect() : null;
  }

  // ── Zoom sulla tratta appena occupata (TV) ──
  const focusCtl = new PZ.FocusController();
  let focusRouteId = $state<string | null>(null);
  let focusRaf = 0;
  // Evento già presente al montaggio (es. rimontaggio della mappa): non si anima.
  let lastFocusSeq: number | null = untrack(() => focusClaim?.seq ?? null);

  function focusFrame(now: number) {
    focusRaf = 0;
    const next = focusCtl.tick(now);
    if (next.panX !== pz.panX || next.panY !== pz.panY || next.zoom !== pz.zoom) pz = next;
    if (focusCtl.phase === 'out' || !focusCtl.active) focusRouteId = null;
    if (focusCtl.active) focusRaf = requestAnimationFrame(focusFrame);
  }

  function startFocus(routeId: string) {
    if (compact) return;
    const pts = layout.routes[routeId];
    const box = pts ? PZ.bboxOfPoints(pts.map(([x, y]) => ({ x, y }))) : null;
    if (!box) return;
    const reduced = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;
    focusCtl.focus(pz, PZ.fitRect(box, vb), performance.now(), reduced);
    focusRouteId = routeId;
    if (!focusRaf) focusRaf = requestAnimationFrame(focusFrame);
  }

  /** L'utente prende il controllo (pan, zoom, +/−): l'animazione si ferma dove sta. */
  function interruptFocus() {
    if (!focusCtl.active) return;
    focusCtl.cancel();
    focusRouteId = null;
    if (focusRaf) { cancelAnimationFrame(focusRaf); focusRaf = 0; }
  }

  $effect(() => {
    const req = focusClaim;
    if (!req || req.seq === lastFocusSeq) return;
    lastFocusSeq = req.seq;
    untrack(() => startFocus(req.routeId));
  });
  $effect(() => () => { if (focusRaf) cancelAnimationFrame(focusRaf); });

  function onDown(e: PointerEvent) {
    interruptFocus();
    if (e.pointerType === 'mouse' && e.button !== 0) return;
    if (gesture.active === 0) {
      const target = e.target as Element | null;
      tapCityId = target?.closest?.('[data-city-id]')?.getAttribute('data-city-id') ?? null;
      tapRouteId = target?.closest?.('[data-route-id]')?.getAttribute('data-route-id') ?? null;
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
    if (e.type === 'pointerup' && gesture.isTap) {
      const cityId = tapCityId;
      const routeId = tapRouteId;
      if (cityId) {
        onCityTap?.(cityId);
      } else if (routeId && interactive && !claimedMap.has(routeId)) {
        onRouteTap?.(routeId);
      }
    }
    tapCityId = null;
    tapRouteId = null;
  }
  function onWheel(e: WheelEvent) {
    interruptFocus();
    e.preventDefault();
    const rect = svgRect();
    if (!rect) return;
    const focus = PZ.clientToViewBox(e.clientX, e.clientY, rect, vb);
    pz = PZ.zoomAt(pz, focus, PZ.wheelFactor(e.deltaY, e.deltaMode), vb);
  }
  function zoomButton(factor: number) {
    interruptFocus();
    pz = PZ.zoomAt(pz, { x: mapWidth / 2, y: mapHeight / 2 }, factor, vb);
  }
  function resetView() {
    interruptFocus();
    pz = { ...PZ.IDENTITY };
  }

  /** Enter/Spazio su una città selezionabile = tap (la mappa è usabile anche da tastiera). */
  function onCityKey(e: KeyboardEvent, cityId: string) {
    if (e.key !== 'Enter' && e.key !== ' ') return;
    e.preventDefault();
    onCityTap?.(cityId);
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

  // Fine partita: rotte del percorso più lungo (scelte dal server), da mettere in risalto.
  const longestSet = $derived(
    new Set(publicStore.state === 'GAME_OVER' ? (publicStore.longestPathRouteIds ?? []) : []),
  );

  // Fine partita (Europe): rotte prese in prestito dalle stazioni, route_id → borrower.
  // Se due giocatori prendono in prestito la stessa rotta vince il primo in ordine
  // di finalScores (deterministico). Vuota fuori da GAME_OVER.
  const borrowedByRoute = $derived.by(() => {
    const m = new Map<string, string>();
    if (publicStore.state !== 'GAME_OVER') return m;
    for (const fs of publicStore.finalScores ?? []) {
      for (const b of fs.borrowedRoutes ?? []) {
        if (!m.has(b.routeId)) m.set(b.routeId, fs.playerId);
      }
    }
    return m;
  });

  // Layer multipli — un pin + una dashed line per OGNI
  // biglietto pinnato (con più destinazioni: un pin su origine e su ciascuna destinazione,
  // una linea dall'origine a ciascuna), colore per indice dalla palette (distingue i
  // tratteggiati quando più biglietti sono pinnati insieme). Decorativo:
  // pointer-events none, il tap-to-claim sulle rotte sottostanti resta
  // intatto. La sparizione al completamento la decide il controller
  // (filtra i biglietti `completed` segnalati dal server): qui zero logica.
  const ticketLayers = $derived(MapUtils.ticketLayers(highlightTickets, cityPositions));

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
  /** Spessore (unità viewBox, prima di `swFactor`) del bordo crema dei vagoni dei giocatori. */
  const WAGON_BORDER = 2;
  /** Cornice scura esterna e quanto la carrozza eccede lo spessore del tassello (idem). */
  const WAGON_OUTLINE = 1.2;
  const WAGON_GROW = 0.8;

  // Alone della tratta appena occupata (zoom TV): geometria e colore del giocatore che l'ha presa.
  const focusGeo = $derived(
    focusRouteId ? routeGeos.find((g) => g.route.id === focusRouteId) : undefined,
  );
  const focusColor = $derived(playerColor(claimedMap.get(focusRouteId ?? '') ?? ''));

  // Stazioni (Europe): segnaposto a casetta nel colore del giocatore, in alto a sinistra del
  // pallino (le stelle dei biglietti stanno in alto a destra). Disegno nostro.
  const STATION_R = $derived(compact ? 7.5 : 8.5);
  function stationPath(r: number): string {
    return `M ${-r} ${r} L ${-r} ${-r * 0.1} L 0 ${-r * 1.1} L ${r} ${-r * 0.1} L ${r} ${r} Z`;
  }
  const stationMarks = $derived(
    publicStore.stations.flatMap((s) => {
      const pos = cityPositions.get(s.city_id);
      return pos ? [{ cityId: s.city_id, x: pos.x, y: pos.y, color: playerColor(s.player_id) }] : [];
    }),
  );

  // Passeggeri (Germany 2017): pallini colorati attorno alla città, dal basso in senso orario.
  const PASSENGER_R = $derived(compact ? 3 : 3.6);
  const passengerMarks = $derived(
    publicStore.passengers.flatMap((cp) => {
      const pos = cityPositions.get(cp.city_id);
      if (!pos) return [];
      const colors = cp.passengers.flatMap((p) => Array<string>(p.count).fill(p.color));
      const ring = cityDotR + PASSENGER_R + 2;
      return colors.map((color, i) => {
        const angle = Math.PI / 2 + (i * Math.PI) / 4;
        return {
          key: `${cp.city_id}-${i}`,
          x: pos.x + ring * Math.cos(angle),
          y: pos.y + ring * Math.sin(angle),
          color: passengerColorCss(color),
        };
      });
    }),
  );
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
      <linearGradient id="{uid}-slot-shade" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="#fff" stop-opacity="0.45" />
        <stop offset="0.45" stop-color="#fff" stop-opacity="0.06" />
        <stop offset="0.55" stop-color="#000" stop-opacity="0" />
        <stop offset="1" stop-color="#000" stop-opacity="0.28" />
      </linearGradient>
      <!-- Pallino città a "bottone" (luce da nord-ovest) -->
      <radialGradient id="{uid}-city-dot" cx="0.38" cy="0.32" r="0.75">
        <stop offset="0" stop-color="#fffdf6" />
        <stop offset="0.6" stop-color="#f3e8cc" />
        <stop offset="1" stop-color="#cdb98f" />
      </radialGradient>
      <!-- Tunnel: tratteggio diagonale sui tasselli (coordinate locali del tassello) -->
      <pattern id="{uid}-tunnel-hatch" width="5" height="5" patternUnits="userSpaceOnUse"
        patternTransform="rotate(45)">
        <line x1="0" y1="0" x2="0" y2="5" stroke="#2a1d0c" stroke-width="1.6" stroke-opacity="0.55" />
      </pattern>
    </defs>

    <g transform="translate({pz.panX}, {pz.panY}) scale({pz.zoom})">
      <!-- 1. Ombre proiettate di tasselli e vagoni (rilievo) -->
      <g class="layer-shadow" aria-hidden="true">
        {#each routeGeos as g (g.route.id)}
          {@const claimed = claimedMap.has(g.route.id)}
          {@const bw = claimed ? WAGON_BORDER * swFactor : 0}
          {@const w = g.sw + (claimed ? (WAGON_GROW + WAGON_OUTLINE) * swFactor + bw : 0)}
          {@const l = claimed ? Math.max(4, g.slotL - bw) + bw + WAGON_OUTLINE * swFactor : g.slotL}
          {#each g.slots as s}
            <rect x={-l / 2} y={-w / 2} width={l} height={w} rx={w * 0.3}
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

      <!-- 3b. Percorso più lungo (fine partita): alone dorato a tre strati, sotto i vagoni -->
      <g class="layer-longest" aria-hidden="true">
        {#each routeGeos as g (g.route.id)}
          {#if longestSet.has(g.route.id)}
            <path d={g.d} fill="none" class="longest-halo"
              stroke-width={g.sw + 20 * swFactor} stroke-linecap="round" stroke-linejoin="round" />
            <path d={g.d} fill="none" class="longest-mid"
              stroke-width={g.sw + 12 * swFactor} stroke-linecap="round" stroke-linejoin="round" />
            <path d={g.d} fill="none" class="longest-core"
              stroke-width={g.sw + 5 * swFactor} stroke-linecap="round" stroke-linejoin="round" />
          {/if}
        {/each}
      </g>

      <!-- 3b2. Rotte prese in prestito dalle stazioni (fine partita): contorno tratteggiato nel colore di chi le usa -->
      <g class="layer-borrowed" aria-hidden="true">
        {#each routeGeos as g (g.route.id)}
          {@const borrower = borrowedByRoute.get(g.route.id)}
          {#if borrower}
            <path d={g.d} fill="none" class="borrowed-base"
              stroke-width={g.sw + 18 * swFactor} stroke-linecap="round" stroke-linejoin="round" />
            <path d={g.d} fill="none" class="borrowed-outline"
              stroke-width={g.sw + 14 * swFactor} stroke-dasharray="10 7" stroke-linejoin="round" />
            <path d={g.d} fill="none" class="borrowed-dash" stroke={playerColor(borrower)}
              stroke-width={g.sw + 8 * swFactor} stroke-dasharray="10 7" stroke-linejoin="round" />
          {/if}
        {/each}
      </g>

      <!-- 3c. Tratta appena occupata (TV, durante lo zoom): alone nel colore del giocatore -->
      {#if focusGeo}
        <g class="layer-focus" aria-hidden="true">
          <path d={focusGeo.d} fill="none" stroke="#fff6dc" opacity="0.7"
            stroke-width={focusGeo.sw + 26} stroke-linecap="round" stroke-linejoin="round" />
          <path d={focusGeo.d} fill="none" stroke={focusColor} class="focus-pulse"
            stroke-width={focusGeo.sw + 20} stroke-linecap="round" stroke-linejoin="round" />
          <path d={focusGeo.d} fill="none" stroke={focusColor} class="focus-pulse"
            stroke-width={focusGeo.sw + 10} stroke-linecap="round" stroke-linejoin="round" />
        </g>
      {/if}

      <!-- 4. Tasselli liberi / vagoni reclamati -->
      <g class="layer-slots">
        {#each routeGeos as g (g.route.id)}
          {@const claimedBy = claimedMap.get(g.route.id)}
          {#if claimedBy}
            {@const color = playerColor(claimedBy)}
            <!-- Treno: carrozza nel colore del giocatore con bordo crema (cornice scura sottile),
                 "tetto" scuro e finestrini sempre visibili — mai scambiabile con una tratta libera. -->
            {@const bw = WAGON_BORDER * swFactor}
            {@const wl = Math.max(4, g.slotL - bw)}
            {@const wh = g.sw + WAGON_GROW * swFactor}
            {@const bodyL = wl - bw}
            {@const bodyH = wh - bw}
            {@const roofH = bodyH * 0.52}
            {@const roofL = Math.max(2, bodyL - 2.4 * swFactor)}
            {@const twin = g.slotL >= 14 * swFactor}
            {@const winH = roofH * 0.7}
            {@const winW = Math.max(1.6, Math.min(6, roofL * (twin ? 0.24 : 0.4)))}
            {#each g.slots as s, i}
              <g transform="translate({s.x} {s.y}) rotate({s.deg})">
                <g class="wagon" style="animation-delay:{i * 70}ms">
                  <rect x={-wl / 2} y={-wh / 2} width={wl} height={wh} rx={wh * 0.28}
                    fill="none" stroke="#1d150c" stroke-width={bw + WAGON_OUTLINE * swFactor} />
                  <rect x={-wl / 2} y={-wh / 2} width={wl} height={wh} rx={wh * 0.28}
                    fill={color} stroke="#fff6dc" stroke-width={bw} />
                  <rect x={-bodyL / 2} y={-bodyH / 2} width={bodyL} height={bodyH} rx={bodyH * 0.2}
                    fill="url(#{uid}-slot-shade)" />
                  <rect x={-roofL / 2} y={-roofH / 2} width={roofL} height={roofH} rx={roofH * 0.3}
                    fill="#1d150c" opacity="0.4" />
                  {#if twin}
                    <rect x={-roofL * 0.27 - winW / 2} y={-winH / 2} width={winW} height={winH} rx="0.8" fill="#fff6dc" />
                    <rect x={roofL * 0.27 - winW / 2} y={-winH / 2} width={winW} height={winH} rx="0.8" fill="#fff6dc" />
                  {:else}
                    <rect x={-winW / 2} y={-winH / 2} width={winW} height={winH} rx="0.8" fill="#fff6dc" />
                  {/if}
                </g>
              </g>
            {/each}
          {:else}
            {@const fill = slotFill(g.route)}
            {@const tunnel = g.route.tunnel === true}
            {@const ferries = g.route.ferries ?? 0}
            {#each g.slots as s, i}
              <g transform="translate({s.x} {s.y}) rotate({s.deg})">
                <rect x={-g.slotL / 2} y={-g.sw / 2} width={g.slotL} height={g.sw} rx={g.sw * 0.22}
                  fill={fill} stroke="#4a3920" stroke-width={1.2 * swFactor} stroke-opacity="0.85" />
                <rect x={-g.slotL / 2} y={-g.sw / 2} width={g.slotL} height={g.sw} rx={g.sw * 0.22}
                  fill="url(#{uid}-slot-shade)" opacity="0.8" />
                {#if tunnel}
                  <rect x={-g.slotL / 2} y={-g.sw / 2} width={g.slotL} height={g.sw} rx={g.sw * 0.22}
                    fill="url(#{uid}-tunnel-hatch)" class="tunnel-hatch" />
                  <rect x={-g.slotL / 2 + 1.2 * swFactor} y={-g.sw / 2 + 1.2 * swFactor}
                    width={g.slotL - 2.4 * swFactor} height={g.sw - 2.4 * swFactor} rx={g.sw * 0.16}
                    fill="none" stroke="#2a1d0c" stroke-width={1.5 * swFactor}
                    stroke-dasharray="{4 * swFactor} {2.5 * swFactor}" class="tunnel-outline" />
                {/if}
                {#if i < ferries}
                  <!-- Traghetto: locomotiva (disegno nostro) sui primi slot, sempre dritta -->
                  {@const k = Math.min((g.slotL - 3 * swFactor) / 48, (g.sw * 0.82) / 22)}
                  <g transform="rotate({s.deg > 90 || s.deg <= -90 ? 180 : 0}) scale({k}) translate(-24 -11)"
                    class="ferry-glyph">
                    <rect x="2" y="8" width="41" height="7" rx="2.6" fill="#2a1d0c" stroke="#fdf7e6" stroke-width="1.6" />
                    <rect x="5.5" y="2.5" width="6" height="5.5" rx="1.4" fill="#2a1d0c" stroke="#fdf7e6" stroke-width="1.2" />
                    <circle cx="16" cy="8" r="3" fill="#2a1d0c" stroke="#fdf7e6" stroke-width="1.2" />
                    <rect x="3" y="15" width="39" height="2.4" rx="1.2" fill="#2a1d0c" />
                    <circle cx="9" cy="18.5" r="2.7" fill="#fdf7e6" stroke="#2a1d0c" stroke-width="1" />
                    <circle cx="19" cy="18.5" r="2.7" fill="#fdf7e6" stroke="#2a1d0c" stroke-width="1" />
                    <circle cx="29" cy="18.5" r="2.7" fill="#fdf7e6" stroke="#2a1d0c" stroke-width="1" />
                    <circle cx="39" cy="18.5" r="2.7" fill="#fdf7e6" stroke="#2a1d0c" stroke-width="1" />
                  </g>
                {/if}
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

      <!-- 5b. Aree di tap delle città selezionabili (sopra quelle delle rotte): raggiungibili anche
           da tastiera (Tab, Enter/Spazio); l'anello di focus compare solo con la tastiera. -->
      {#if cityInteractive}
        <g class="layer-hit">
          {#each mapData.cities as city (city.id)}
            {@const pos = cityPositions.get(city.id)}
            {#if pos && selectableCitySet.has(city.id)}
              <g class="city-tappable" role="button" tabindex="0"
                aria-label={cityLabel(city)} aria-pressed={city.id === selectedCityId}
                onkeydown={(e) => onCityKey(e, city.id)}>
                <circle cx={pos.x} cy={pos.y} r={cityDotR + 14} fill="none" stroke="#2a1d0c"
                  stroke-width={7 * swFactor} class="city-focus" pointer-events="none" />
                <circle cx={pos.x} cy={pos.y} r={cityDotR + 14} fill="none" stroke="#fdf7e6"
                  stroke-width={3.5 * swFactor} class="city-focus" pointer-events="none" />
                <circle cx={pos.x} cy={pos.y} r={cityDotR + 10} data-city-id={city.id}
                  fill="#000" fill-opacity="0" pointer-events="all" />
              </g>
            {/if}
          {/each}
        </g>
      {/if}

      <!-- 6. Linee dei biglietti pinnati (sotto le città) -->
      {#each ticketLayers as layer}
        {#each layer.paths as d}
          <path {d} class="ticket-line ticket-highlight" aria-hidden="true"
            fill="none" stroke={layer.color} stroke-width={3.5 * swFactor}
            stroke-linecap="round" stroke-dasharray="10 7" opacity="0.9" />
        {/each}
      {/each}

      <!-- 7. Città: ombra, pallino a bottone, anello doppio per gli hub.
           Ogni città è un'immagine col suo nome per gli screen reader (quelle selezionabili
           sono già esposte come pulsanti al livello 5b). -->
      <g class="layer-cities">
        <g aria-hidden="true">
          {#each labelLeaders as l (l.id)}
            <line x1={l.x1} y1={l.y1} x2={l.x2} y2={l.y2}
              stroke="#453416" stroke-width="1.1" stroke-opacity="0.55" stroke-linecap="round" />
          {/each}
        </g>
        {#each mapData.cities as city (city.id)}
          {@const pos = cityPositions.get(city.id)}
          {#if pos}
            {@const hub = !city.country && (cityDegree.get(city.id) ?? 0) >= 6}
            {@const exposedAsButton = cityInteractive && selectableCitySet.has(city.id)}
            <g role={exposedAsButton ? undefined : 'img'}
              aria-label={exposedAsButton ? undefined : cityLabel(city)}
              aria-hidden={exposedAsButton ? 'true' : undefined}>
              {#if city.country}
                <!-- Paese confinante (vicolo cieco): targhetta quadrata con bandierina, non un pallino -->
                {@const h = cityDotR}
                <rect x={pos.x - h + SHADOW_DX} y={pos.y - h + SHADOW_DY} width={2 * h} height={2 * h}
                  rx="3" fill="#2a1d0c" opacity="0.25" />
                <rect x={pos.x - h} y={pos.y - h} width={2 * h} height={2 * h} rx="3"
                  fill="#e3cf9c" stroke="#453416" stroke-width="2" class="country-badge" />
                <path d="M {pos.x - h * 0.35} {pos.y + h * 0.62} V {pos.y - h * 0.62} L {pos.x + h * 0.55} {pos.y - h * 0.22} L {pos.x - h * 0.35} {pos.y + h * 0.18}"
                  fill="#453416" fill-opacity="0.75" stroke="#453416" stroke-width="1.4"
                  stroke-linejoin="round" stroke-linecap="round" />
              {:else}
                <circle cx={pos.x + SHADOW_DX} cy={pos.y + SHADOW_DY} r={cityDotR + (hub ? 1.5 : 0)}
                  fill="#2a1d0c" opacity="0.25" />
                <circle cx={pos.x} cy={pos.y} r={cityDotR + (hub ? 1.5 : 0)} fill="url(#{uid}-city-dot)"
                  stroke="#453416" stroke-width="2" />
              {/if}
              {#if hub}
                <circle cx={pos.x} cy={pos.y} r={cityDotR * 0.45} fill="#453416" opacity="0.75" />
              {/if}
              {#if bigCitySet.has(city.id)}
                <circle cx={pos.x} cy={pos.y} r={cityDotR + (hub ? 1.5 : 0) + 3.5} fill="none"
                  class="big-city-ring" stroke-width={2.4 * swFactor} pointer-events="none" />
              {/if}
            </g>
          {/if}
        {/each}
      </g>

      <!-- 7b. Città selezionabili (anello) e stazioni costruite (segnaposto nel colore del giocatore) -->
      <g class="layer-stations" aria-hidden="true" pointer-events="none">
        {#each mapData.cities as city (city.id)}
          {@const pos = cityPositions.get(city.id)}
          {#if pos && selectableCitySet.has(city.id)}
            {@const picked = city.id === selectedCityId}
            <!-- Alone + anello con contorno scuro: leggibile sulla pergamena anche sul telefono. -->
            <circle cx={pos.x} cy={pos.y} r={cityDotR + 10} fill={picked ? '#d35400' : '#e8a83e'}
              fill-opacity={picked ? 0.4 : 0.28} class={picked ? 'city-halo picked' : 'city-halo'} />
            <circle cx={pos.x} cy={pos.y} r={cityDotR + 6} fill="none"
              stroke="#2a1d0c" stroke-width={(picked ? 7 : 6) * swFactor} />
            <circle cx={pos.x} cy={pos.y} r={cityDotR + 6} fill="none"
              stroke={picked ? '#d35400' : '#f5b942'} stroke-width={(picked ? 4.5 : 3.5) * swFactor}
              class={picked ? 'city-ring picked' : 'city-ring'} />
          {/if}
        {/each}
        {#each stationMarks as st (st.cityId)}
          <g transform="translate({st.x - cityDotR - STATION_R * 0.9} {st.y - cityDotR - STATION_R * 0.9})">
            <path d={stationPath(STATION_R)} transform="translate({SHADOW_DX * 0.6} {SHADOW_DY * 0.6})"
              fill="#2a1d0c" opacity="0.35" />
            <path d={stationPath(STATION_R)} fill={st.color} stroke="#2a1d0c"
              stroke-width={4.4 * swFactor} stroke-linejoin="round" />
            <path d={stationPath(STATION_R)} fill={st.color} stroke="#fdf7e6"
              stroke-width={2.2 * swFactor} stroke-linejoin="round" />
            <rect x={-STATION_R * 0.22} y={STATION_R * 0.1} width={STATION_R * 0.44} height={STATION_R * 0.9}
              fill="#fdf7e6" opacity="0.85" />
          </g>
        {/each}
        {#each passengerMarks as pm (pm.key)}
          <circle cx={pm.x} cy={pm.y} r={PASSENGER_R} fill={pm.color}
            stroke="#2a1d0c" stroke-width={1.2 * swFactor} />
        {/each}
      </g>

      <!-- 8. Pin biglietti: anello attorno al pallino + stella a lato
           (non coprono più il pallino della città). -->
      {#each ticketLayers as layer}
        <g class="ticket-highlight" aria-hidden="true">
          {#each layer.pins as pinPos}
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
  .layer-shadow, .layer-bed, .layer-glow, .layer-focus, .layer-longest, .layer-borrowed, .layer-slots, .layer-cities, .layer-labels {
    pointer-events: none;
  }
  /* Alone della tratta appena occupata: pulsa solo senza preferenza di movimento ridotto. */
  .focus-pulse { opacity: 0.6; }
  @media (prefers-reduced-motion: no-preference) {
    .focus-pulse { animation: focus-pulse 0.9s ease-in-out infinite alternate; }
  }
  @keyframes focus-pulse {
    from { opacity: 0.3; }
    to { opacity: 0.85; }
  }
  /* Percorso più lungo (fine partita): alone dorato, sempre visibile; pulsa solo senza
     preferenza di movimento ridotto. Colori dai token (--gold*). */
  .big-city-ring { stroke: var(--gold-deep); }
  .longest-halo { stroke: var(--gold); opacity: 0.28; }
  .longest-mid { stroke: var(--gold-bright); opacity: 0.5; }
  .longest-core { stroke: var(--gold-bright); opacity: 0.95; }
  @media (prefers-reduced-motion: no-preference) {
    .longest-halo, .longest-mid { animation: longest-pulse 1.4s ease-in-out infinite alternate; }
  }
  @keyframes longest-pulse {
    from { opacity: 0.15; }
    to { opacity: 0.6; }
  }
  /* Rotte in prestito (fine partita): base crema + contorno scuro + tratteggio nel colore
     del giocatore; il tratteggio scorre solo senza preferenza di movimento ridotto. */
  .borrowed-base { stroke: #fff6dc; opacity: 0.6; }
  .borrowed-outline { stroke: #2b1d10; opacity: 0.85; }
  .borrowed-dash { opacity: 0.95; }
  @media (prefers-reduced-motion: no-preference) {
    .borrowed-outline, .borrowed-dash { animation: dash-march 1.6s linear infinite; }
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
  .tunnel-hatch, .tunnel-outline, .ferry-glyph { pointer-events: none; }
  .city-tappable { cursor: pointer; outline: none; }
  .city-focus { display: none; }
  .city-tappable:focus-visible .city-focus { display: inline; }
  .city-ring { opacity: 1; }
  @media (prefers-reduced-motion: no-preference) {
    .city-ring:not(.picked), .city-halo:not(.picked) { animation: glow-pulse 1.2s ease-in-out infinite alternate; }
  }
</style>
