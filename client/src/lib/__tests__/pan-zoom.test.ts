import { describe, it, expect } from 'vitest';
import {
  clientToViewBox, cssPerUnit, clamp, zoomAt, panBy, wheelFactor, GestureTracker, IDENTITY,
  bboxOfPoints, fitRect, easeInOutCubic, lerpPanZoom, FocusController, MAX_ZOOM,
} from '../pan-zoom.ts';

const vb = { width: 1000, height: 500 };

describe('clientToViewBox', () => {
  it('letterbox orizzontale (rect più largo)', () => {
    const rect = { left: 10, top: 20, width: 400, height: 100 }; // scala 0.2, offX 100
    expect(cssPerUnit(rect, vb)).toBeCloseTo(0.2);
    const p = clientToViewBox(10 + 100 + 20, 20 + 10, rect, vb);
    expect(p.x).toBeCloseTo(100);
    expect(p.y).toBeCloseTo(50);
  });
  it('letterbox verticale (rect più alto)', () => {
    const rect = { left: 0, top: 0, width: 100, height: 300 }; // scala 0.1, offY 125
    const p = clientToViewBox(50, 125 + 25, rect, vb);
    expect(p.x).toBeCloseTo(500);
    expect(p.y).toBeCloseTo(250);
  });
});

describe('clamp / zoomAt / panBy', () => {
  it('a zoom 1 il pan è 0', () => {
    expect(clamp({ panX: 50, panY: -30, zoom: 0.5 }, vb)).toEqual({ panX: 0, panY: 0, zoom: 1 });
  });
  it('limiti a zoom 2', () => {
    expect(clamp({ panX: -5000, panY: 100, zoom: 2 }, vb)).toEqual({ panX: -1000, panY: 0, zoom: 2 });
    expect(clamp({ panX: 1, panY: -9999, zoom: 9 }, vb)).toEqual({ panX: 0, panY: -2000, zoom: 5 });
  });
  it('zoomAt mantiene fisso il fuoco', () => {
    const s = { panX: -200, panY: -100, zoom: 2 };
    const f = { x: 400, y: 200 };
    const before = { x: (f.x - s.panX) / s.zoom, y: (f.y - s.panY) / s.zoom };
    const n = zoomAt(s, f, 1.5, vb);
    expect(n.zoom).toBeCloseTo(3);
    expect((f.x - n.panX) / n.zoom).toBeCloseTo(before.x);
    expect((f.y - n.panY) / n.zoom).toBeCloseTo(before.y);
  });
  it('panBy applica clamp', () => {
    expect(panBy(IDENTITY, 10, 10, vb)).toEqual({ panX: 0, panY: 0, zoom: 1 });
  });
});

describe('GestureTracker', () => {
  const rect = { left: 0, top: 0, width: 300, height: 150 }; // cssPerUnit 0.3
  const start = { panX: -500, panY: -250, zoom: 2 };

  it('pan a un dito converte px CSS in unità viewBox', () => {
    const g = new GestureTracker();
    g.down(1, 100, 50);
    const n = g.move(1, 130, 50, start, rect, vb);
    expect(n.panX).toBeCloseTo(-400);
    expect(n.panY).toBeCloseTo(-250);
    expect(g.isTap).toBe(false);
  });

  it('pinch: dita che si allargano raddoppiano lo zoom', () => {
    const g = new GestureTracker();
    const s0 = { panX: 0, panY: 0, zoom: 1 };
    g.down(1, 140, 75);
    g.down(2, 160, 75);
    const n = g.move(2, 180, 75, s0, rect, vb); // dist 20 -> 40
    expect(n.zoom).toBeCloseTo(2);
    expect(g.isTap).toBe(false);
  });

  it('passaggio 2→1 dita senza salti', () => {
    const g = new GestureTracker();
    g.down(1, 100, 50);
    g.down(2, 200, 50);
    let s = g.move(2, 220, 50, start, rect, vb);
    g.up(2);
    expect(g.active).toBe(1);
    const n = g.move(1, 100, 50, s, rect, vb); // nessuno spostamento reale
    expect(n).toEqual(s);
    s = g.move(1, 103, 50, n, rect, vb);
    expect(s.panX - n.panX).toBeCloseTo(10);
  });

  it('isTap', () => {
    const g = new GestureTracker();
    g.down(1, 0, 0);
    g.move(1, 3, 3, IDENTITY, rect, vb);
    g.up(1);
    expect(g.isTap).toBe(true);
    g.down(1, 0, 0);
    g.move(1, 20, 0, IDENTITY, rect, vb);
    expect(g.isTap).toBe(false);
    g.up(1);
    g.down(1, 0, 0);
    expect(g.isTap).toBe(true);
    g.down(2, 5, 5);
    expect(g.isTap).toBe(false);
  });
});

describe('wheelFactor', () => {
  it('segno e limiti', () => {
    expect(wheelFactor(100)).toBeLessThan(1);
    expect(wheelFactor(-100)).toBeGreaterThan(1);
    expect(wheelFactor(10000)).toBe(0.5);
    expect(wheelFactor(3, 1)).toBeCloseTo(Math.exp(-48 * 0.0015));
  });
});

describe('bboxOfPoints / fitRect', () => {
  it('bbox dei punti, null se vuota', () => {
    expect(bboxOfPoints([])).toBeNull();
    expect(bboxOfPoints([{ x: 10, y: 40 }, { x: 30, y: 20 }, { x: 25, y: 50 }]))
      .toEqual({ x: 10, y: 20, width: 20, height: 30 });
  });

  it('centra il rettangolo con zoom limitato a 2.5 per le tratte corte', () => {
    const v = fitRect({ x: 480, y: 230, width: 40, height: 40 }, vb);
    expect(v.zoom).toBe(2.5);
    // il centro (500, 250) cade al centro del viewBox
    expect(500 * v.zoom + v.panX).toBeCloseTo(500);
    expect(250 * v.zoom + v.panY).toBeCloseTo(250);
  });

  it('tratte lunghe: zoom minore (ma >= 1.6) perché stiano per intero', () => {
    const v = fitRect({ x: 300, y: 200, width: 400, height: 100 }, vb, { margin: 50 });
    expect(v.zoom).toBeCloseTo(1000 / 500);
    const huge = fitRect({ x: 0, y: 0, width: 1000, height: 500 }, vb);
    expect(huge.zoom).toBe(1.6);
  });

  it('zoom mai oltre MAX_ZOOM e vista sempre valida (clamp ai bordi)', () => {
    const corner = fitRect({ x: 0, y: 0, width: 10, height: 10 }, vb, { maxZoom: 99, margin: 5 });
    expect(corner.zoom).toBe(MAX_ZOOM);
    expect(corner).toEqual(clamp(corner, vb));
    const edge = fitRect({ x: 960, y: 470, width: 30, height: 20 }, vb);
    expect(edge).toEqual(clamp(edge, vb));
    // il rettangolo resta visibile: x in [0, 1000] dopo la trasformazione
    const sx = 975 * edge.zoom + edge.panX;
    expect(sx).toBeGreaterThan(0);
    expect(sx).toBeLessThanOrEqual(1000);
  });
});

describe('easeInOutCubic / lerpPanZoom', () => {
  it('estremi e simmetria', () => {
    expect(easeInOutCubic(0)).toBe(0);
    expect(easeInOutCubic(1)).toBe(1);
    expect(easeInOutCubic(0.5)).toBeCloseTo(0.5);
    expect(easeInOutCubic(-3)).toBe(0);
    expect(easeInOutCubic(9)).toBe(1);
  });
  it('interpola pan e zoom; i punti intermedi di due viste valide sono validi', () => {
    const a = { panX: 0, panY: 0, zoom: 1 };
    const b = { panX: -400, panY: -200, zoom: 3 };
    expect(lerpPanZoom(a, b, 0)).toEqual(a);
    expect(lerpPanZoom(a, b, 1)).toEqual(b);
    expect(lerpPanZoom(a, b, 0.5)).toEqual({ panX: -200, panY: -100, zoom: 2 });
    const edge = fitRect({ x: 900, y: 450, width: 40, height: 30 }, vb);
    for (const t of [0.1, 0.3, 0.7, 0.9]) {
      const m = lerpPanZoom(a, edge, t);
      const c = clamp(m, vb);
      expect(c.zoom).toBeCloseTo(m.zoom);
      expect(c.panX).toBeCloseTo(m.panX);
      expect(c.panY).toBeCloseTo(m.panY);
    }
  });
});

describe('FocusController', () => {
  const timing = { inMs: 100, holdMs: 1000, outMs: 100 };
  const target = { panX: -500, panY: -250, zoom: 2 };

  it('zoom-in, pausa, ritorno alla vista precedente (non per forza l’identità)', () => {
    const before = { panX: -100, panY: -50, zoom: 1.5 };
    const fc = new FocusController(timing);
    expect(fc.phase).toBe('idle');
    fc.focus(before, target, 0);
    expect(fc.phase).toBe('in');
    expect(fc.tick(0)).toEqual(before);
    const mid = fc.tick(50);
    expect(mid.zoom).toBeGreaterThan(1.5);
    expect(mid.zoom).toBeLessThan(2);
    expect(fc.tick(100)).toEqual(target);
    expect(fc.phase).toBe('hold');
    expect(fc.tick(1000)).toEqual(target);
    expect(fc.tick(1100)).toEqual(target); // fine pausa: inizia il ritorno
    expect(fc.phase).toBe('out');
    expect(fc.tick(1150).zoom).toBeLessThan(2);
    expect(fc.tick(1200)).toEqual(before);
    expect(fc.phase).toBe('idle');
    expect(fc.active).toBe(false);
  });

  it('un secondo focus durante l’animazione riparte dalla vista corrente e mantiene la base', () => {
    const fc = new FocusController(timing);
    fc.focus(IDENTITY, target, 0);
    fc.tick(100);
    fc.tick(500); // in pausa sul primo bersaglio
    const second = { panX: -200, panY: -100, zoom: 2.5 };
    // `current` è ignorata: l'animazione è già attiva
    fc.focus({ panX: 123, panY: 456, zoom: 4 }, second, 500);
    expect(fc.tick(500)).toEqual(target); // nessun salto
    expect(fc.tick(600)).toEqual(second);
    expect(fc.phase).toBe('hold');
    // la pausa riparte dal secondo claim
    expect(fc.tick(1500)).toEqual(second);
    fc.tick(1600);
    expect(fc.tick(1700)).toEqual(IDENTITY);
    expect(fc.active).toBe(false);
  });

  it('un focus durante il ritorno riparte dal punto raggiunto', () => {
    const fc = new FocusController(timing);
    fc.focus(IDENTITY, target, 0);
    fc.tick(1100);
    const mid = fc.tick(1150);
    fc.focus(IDENTITY, target, 1150);
    expect(fc.tick(1150)).toEqual(mid);
    expect(fc.tick(1250)).toEqual(target);
  });

  it('movimento ridotto: salta senza interpolare, tiene la pausa e torna di colpo', () => {
    const fc = new FocusController(timing);
    fc.focus(IDENTITY, target, 0, true);
    expect(fc.tick(0)).toEqual(target);
    expect(fc.phase).toBe('hold');
    expect(fc.tick(999)).toEqual(target);
    expect(fc.tick(1000)).toEqual(IDENTITY);
    expect(fc.phase).toBe('idle');
  });

  it('cancel lascia la vista dove sta e ferma tutto', () => {
    const fc = new FocusController(timing);
    fc.focus(IDENTITY, target, 0);
    const mid = fc.tick(50);
    fc.cancel();
    expect(fc.active).toBe(false);
    expect(fc.tick(5000)).toEqual(mid);
  });

  it('frame in ritardo: recupera le fasi senza scatti', () => {
    const fc = new FocusController(timing);
    fc.focus(IDENTITY, target, 0);
    expect(fc.tick(10_000)).toEqual(IDENTITY);
    expect(fc.active).toBe(false);
  });
});
