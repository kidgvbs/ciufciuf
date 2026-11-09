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
