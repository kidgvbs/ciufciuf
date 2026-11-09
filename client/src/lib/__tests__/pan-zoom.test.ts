import { describe, it, expect } from 'vitest';
import {
  clientToViewBox, cssPerUnit, clamp, zoomAt, panBy, wheelFactor, GestureTracker, IDENTITY,
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
