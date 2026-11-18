// Logica pura di pan/zoom per la mappa SVG (nessuna dipendenza dal framework).
// Tutte le coordinate di PanZoom sono in unità del viewBox:
// transform = translate(panX, panY) scale(zoom).

export interface PanZoom { panX: number; panY: number; zoom: number }
export interface Size { width: number; height: number }
export interface RectLike { left: number; top: number; width: number; height: number }

export const MIN_ZOOM = 1;
export const MAX_ZOOM = 5;
export const IDENTITY: PanZoom = Object.freeze({ panX: 0, panY: 0, zoom: 1 });

/** Pixel CSS per unità di viewBox (scala "meet"). */
export function cssPerUnit(rect: RectLike, vb: Size): number {
  return Math.min(rect.width / vb.width, rect.height / vb.height);
}

/** Punto client CSS -> punto viewBox, rispettando xMidYMid meet (letterbox su entrambi gli assi). */
export function clientToViewBox(clientX: number, clientY: number, rect: RectLike, vb: Size): { x: number; y: number } {
  const s = cssPerUnit(rect, vb);
  // Offset del letterbox: il contenuto è centrato nel rettangolo
  const offX = (rect.width - vb.width * s) / 2;
  const offY = (rect.height - vb.height * s) / 2;
  return {
    x: (clientX - rect.left - offX) / s,
    y: (clientY - rect.top - offY) / s,
  };
}

function clampNum(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

/** Limita zoom e pan in modo che il contenuto copra sempre il viewBox. */
export function clamp(s: PanZoom, vb: Size): PanZoom {
  const zoom = clampNum(Number.isFinite(s.zoom) ? s.zoom : 1, MIN_ZOOM, MAX_ZOOM);
  const panX = clampNum(s.panX, vb.width * (1 - zoom), 0);
  const panY = clampNum(s.panY, vb.height * (1 - zoom), 0);
  // Normalizza -0 a 0
  return { panX: panX + 0, panY: panY + 0, zoom };
}

/** Zoom di `factor` mantenendo fisso il punto `focus` (coord. viewBox), poi clamp. */
export function zoomAt(s: PanZoom, focus: { x: number; y: number }, factor: number, vb: Size): PanZoom {
  const zoom = clampNum(s.zoom * factor, MIN_ZOOM, MAX_ZOOM);
  const k = zoom / s.zoom;
  return clamp({
    panX: focus.x - (focus.x - s.panX) * k,
    panY: focus.y - (focus.y - s.panY) * k,
    zoom,
  }, vb);
}

/** Pan di un delta in unità viewBox, poi clamp. */
export function panBy(s: PanZoom, dx: number, dy: number, vb: Size): PanZoom {
  return clamp({ panX: s.panX + dx, panY: s.panY + dy, zoom: s.zoom }, vb);
}

/** Fattore di zoom dalla rotella (deltaY>0 = zoom out). deltaMode 1 = righe. */
export function wheelFactor(deltaY: number, deltaMode = 0): number {
  const px = deltaMode === 1 ? deltaY * 16 : deltaY;
  return clampNum(Math.exp(-px * 0.0015), 0.5, 2);
}

// ── Zoom animato su una zona (TV: tratta appena occupata) ─────────────────

export interface Rect { x: number; y: number; width: number; height: number }

/** Bounding box di una lista di punti (coordinate viewBox); `null` se vuota. */
export function bboxOfPoints(points: ReadonlyArray<{ x: number; y: number }>): Rect | null {
  if (points.length === 0) return null;
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const p of points) {
    minX = Math.min(minX, p.x); maxX = Math.max(maxX, p.x);
    minY = Math.min(minY, p.y); maxY = Math.max(maxY, p.y);
  }
  return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
}

export interface FitOptions {
  /** Margine attorno al rettangolo, in unità viewBox. */
  margin?: number;
  /** Zoom minimo/massimo desiderato (comunque limitato a [MIN_ZOOM, MAX_ZOOM]). */
  minZoom?: number;
  maxZoom?: number;
}

/**
 * Vista (pan+zoom) che centra `bbox` nel viewBox: zoom = quello che lo fa stare per intero con il
 * margine, limitato a [minZoom, maxZoom]; poi clamp (vicino ai bordi il centro si sposta, il
 * rettangolo resta comunque visibile).
 */
export function fitRect(bbox: Rect, vb: Size, opts: FitOptions = {}): PanZoom {
  const margin = opts.margin ?? 60;
  const lo = clampNum(opts.minZoom ?? 1.6, MIN_ZOOM, MAX_ZOOM);
  const hi = clampNum(opts.maxZoom ?? 2.5, lo, MAX_ZOOM);
  const fit = Math.min(vb.width / (bbox.width + 2 * margin), vb.height / (bbox.height + 2 * margin));
  const zoom = clampNum(Number.isFinite(fit) ? fit : hi, lo, hi);
  const cx = bbox.x + bbox.width / 2;
  const cy = bbox.y + bbox.height / 2;
  return clamp({ panX: vb.width / 2 - cx * zoom, panY: vb.height / 2 - cy * zoom, zoom }, vb);
}

/** Easing in/out cubico, t in [0,1]. */
export function easeInOutCubic(t: number): number {
  const x = clampNum(t, 0, 1);
  return x < 0.5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2;
}

/**
 * Interpolazione lineare di pan e zoom. Se `a` e `b` sono viste valide lo è anche ogni punto
 * intermedio (il vincolo del clamp è lineare in zoom e pan), quindi nessun glitch ai bordi.
 */
export function lerpPanZoom(a: PanZoom, b: PanZoom, t: number): PanZoom {
  return {
    panX: a.panX + (b.panX - a.panX) * t,
    panY: a.panY + (b.panY - a.panY) * t,
    zoom: a.zoom + (b.zoom - a.zoom) * t,
  };
}

export interface FocusTiming { inMs: number; holdMs: number; outMs: number }
export const DEFAULT_FOCUS_TIMING: FocusTiming = Object.freeze({ inMs: 800, holdMs: 2500, outMs: 900 });

export type FocusPhase = 'idle' | 'in' | 'hold' | 'out';

/**
 * Macchina a stati del "zoom sulla mossa": zoom-in → pausa → ritorno alla vista di partenza.
 * Pura (il tempo arriva da fuori, es. `requestAnimationFrame`). Un nuovo `focus` durante
 * l'animazione riparte dalla vista corrente verso il nuovo bersaglio SENZA toccare la vista da
 * ripristinare (resta quella di prima del primo zoom): niente scatti e niente drift.
 */
export class FocusController {
  private readonly timing: FocusTiming;
  private _phase: FocusPhase = 'idle';
  private base: PanZoom = { ...IDENTITY };
  private from: PanZoom = { ...IDENTITY };
  private to: PanZoom = { ...IDENTITY };
  private view: PanZoom = { ...IDENTITY };
  private start = 0;
  private instant = false;

  constructor(timing: FocusTiming = DEFAULT_FOCUS_TIMING) {
    this.timing = timing;
  }

  get phase(): FocusPhase { return this._phase; }
  get active(): boolean { return this._phase !== 'idle'; }

  /**
   * Avvia (o ridirige) lo zoom su `target`. `current` è la vista attuale (usata come vista da
   * ripristinare se l'animazione era ferma). `instant`: niente interpolazione (movimento ridotto).
   */
  focus(current: PanZoom, target: PanZoom, now: number, instant = false): void {
    if (this._phase === 'idle') {
      this.base = { ...current };
      this.from = { ...current };
      this.view = { ...current };
    } else {
      this.from = { ...this.view };
    }
    this.to = { ...target };
    this.instant = instant;
    this._phase = 'in';
    this.start = now;
  }

  /** Ferma l'animazione lasciando la vista dove sta (l'utente ha preso il controllo). */
  cancel(): void {
    this._phase = 'idle';
  }

  /** Vista da mostrare al tempo `now`. */
  tick(now: number): PanZoom {
    const { inMs, holdMs, outMs } = this.timing;
    for (;;) {
      switch (this._phase) {
        case 'idle':
          return this.view;
        case 'in': {
          const dur = this.instant ? 0 : inMs;
          const t = dur <= 0 ? 1 : (now - this.start) / dur;
          if (t >= 1) {
            this.view = { ...this.to };
            this._phase = 'hold';
            this.start += dur;
            continue;
          }
          this.view = lerpPanZoom(this.from, this.to, easeInOutCubic(Math.max(0, t)));
          return this.view;
        }
        case 'hold':
          if (now - this.start >= holdMs) {
            this.from = { ...this.to };
            this.to = { ...this.base };
            this._phase = 'out';
            this.start += holdMs;
            continue;
          }
          this.view = { ...this.to };
          return this.view;
        case 'out': {
          const dur = this.instant ? 0 : outMs;
          const t = dur <= 0 ? 1 : (now - this.start) / dur;
          if (t >= 1) {
            this.view = { ...this.base };
            this._phase = 'idle';
            return this.view;
          }
          this.view = lerpPanZoom(this.from, this.to, easeInOutCubic(Math.max(0, t)));
          return this.view;
        }
      }
    }
  }
}

interface Pt { x: number; y: number }

/** Traccia fino a 2 puntatori: 1 = pan, 2 = pinch zoom + pan del punto medio. */
export class GestureTracker {
  private readonly tapSlopPx: number;
  // Ultima posizione nota (CSS client) per ogni puntatore attivo, in ordine di arrivo
  private pointers = new Map<number, Pt>();
  private origin: Pt | null = null;
  private tap = true;

  constructor(opts?: { tapSlopPx?: number }) {
    this.tapSlopPx = opts?.tapSlopPx ?? 10;
  }

  get active(): number { return this.pointers.size; }
  get isTap(): boolean { return this.tap; }

  down(id: number, clientX: number, clientY: number): void {
    if (this.pointers.size === 0) {
      // Nuovo gesto
      this.tap = true;
      this.origin = { x: clientX, y: clientY };
    } else {
      this.tap = false;
    }
    if (this.pointers.size >= 2) return; // oltre 2 ignorati
    this.pointers.set(id, { x: clientX, y: clientY });
  }

  move(id: number, clientX: number, clientY: number, s: PanZoom, rect: RectLike, vb: Size): PanZoom {
    const prev = this.pointers.get(id);
    if (!prev) return s;
    const cur = { x: clientX, y: clientY };

    if (this.pointers.size === 1) {
      if (this.origin && Math.hypot(cur.x - this.origin.x, cur.y - this.origin.y) >= this.tapSlopPx) {
        this.tap = false;
      }
      this.pointers.set(id, cur);
      const k = cssPerUnit(rect, vb);
      return panBy(s, (cur.x - prev.x) / k, (cur.y - prev.y) / k, vb);
    }

    // Pinch incrementale: confronta la coppia prima/dopo questo movimento
    let other: Pt | undefined;
    for (const [pid, p] of this.pointers) if (pid !== id) other = p;
    this.pointers.set(id, cur);
    if (!other) return s;

    const m0 = clientToViewBox((prev.x + other.x) / 2, (prev.y + other.y) / 2, rect, vb);
    const m1 = clientToViewBox((cur.x + other.x) / 2, (cur.y + other.y) / 2, rect, vb);
    const d0 = Math.hypot(prev.x - other.x, prev.y - other.y);
    const d1 = Math.hypot(cur.x - other.x, cur.y - other.y);
    const factor = d0 > 0 && d1 > 0 ? d1 / d0 : 1;

    // Punto del contenuto sotto il vecchio punto medio
    const cx = (m0.x - s.panX) / s.zoom;
    const cy = (m0.y - s.panY) / s.zoom;
    const zoom = clampNum(s.zoom * factor, MIN_ZOOM, MAX_ZOOM);
    // Lo posiziona sotto il nuovo punto medio
    return clamp({ panX: m1.x - cx * zoom, panY: m1.y - cy * zoom, zoom }, vb);
  }

  up(id: number): void {
    // Le posizioni sono sempre aggiornate: il puntatore restante riparte dalla sua ultima posizione (nessun salto)
    this.pointers.delete(id);
  }
}
