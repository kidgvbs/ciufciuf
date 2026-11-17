import { describe, it, expect } from 'vitest';
import {
  DEFAULT_DOUBLE_GAP,
  doubleRouteOffset,
  groupDoubleRoutes,
  routeStrokeWidth,
  type RouteData,
} from '../map-utils';

const route = (id: string, from: string, to: string, extra: Partial<RouteData> = {}): RouteData => ({
  id,
  from,
  to,
  len: 3,
  color: 'Gray',
  double: false,
  ...extra,
});

describe('doubleRouteOffset: gruppi paralleli', () => {
  it('con 2 binari resta ±(metà spessore + metà gap), come prima', () => {
    for (const len of [1, 2, 3, 4, 6]) {
      const mag = routeStrokeWidth(len) / 2 + DEFAULT_DOUBLE_GAP / 2;
      expect(doubleRouteOffset(len, 0)).toBe(mag);
      expect(doubleRouteOffset(len, 1)).toBe(-mag);
      expect(doubleRouteOffset(len, 0, DEFAULT_DOUBLE_GAP, 2)).toBe(mag);
      expect(doubleRouteOffset(len, 1, DEFAULT_DOUBLE_GAP, 2)).toBe(-mag);
    }
  });

  it('con 3 binari: +passo, 0, −passo', () => {
    const pitch = routeStrokeWidth(1) + DEFAULT_DOUBLE_GAP;
    expect(doubleRouteOffset(1, 0, DEFAULT_DOUBLE_GAP, 3)).toBe(pitch);
    expect(doubleRouteOffset(1, 1, DEFAULT_DOUBLE_GAP, 3)).toBe(0);
    expect(doubleRouteOffset(1, 2, DEFAULT_DOUBLE_GAP, 3)).toBe(-pitch);
  });

  it('con 4 binari: ±1.5·passo e ±0.5·passo, simmetrici e equispaziati', () => {
    const len = 2;
    const pitch = routeStrokeWidth(len) + 3;
    const offs = [0, 1, 2, 3].map((i) => doubleRouteOffset(len, i, 3, 4));
    expect(offs).toEqual([1.5 * pitch, 0.5 * pitch, -0.5 * pitch, -1.5 * pitch]);
    for (let i = 0; i + 1 < offs.length; i++) expect(offs[i]! - offs[i + 1]!).toBeCloseTo(pitch, 10);
  });

  it('la somma degli offset di ogni gruppo è zero (centrati sulla linea)', () => {
    for (let n = 2; n <= 4; n++) {
      let sum = 0;
      for (let i = 0; i < n; i++) sum += doubleRouteOffset(4, i, 5, n);
      expect(sum).toBeCloseTo(0, 10);
    }
  });
});

describe('groupDoubleRoutes: 2–4 binari', () => {
  it('indicizza i binari in ordine e riporta la dimensione del gruppo, anche a coppia invertita', () => {
    const routes = [
      route('a_b_1', 'a', 'b', { double: true }),
      route('a_b_2', 'a', 'b', { double: true }),
      route('a_b_3', 'b', 'a', { double: true }),
      route('c_d_1', 'c', 'd', { double: true }),
      route('c_d_2', 'c', 'd', { double: true }),
      route('e_f_1', 'e', 'f'),
    ];
    const { routeIndex, routeGroupSize, groups } = groupDoubleRoutes(routes);
    expect([...routeIndex]).toEqual([
      ['a_b_1', 0], ['a_b_2', 1], ['a_b_3', 2], ['c_d_1', 0], ['c_d_2', 1], ['e_f_1', 0],
    ]);
    expect(routeGroupSize.get('a_b_3')).toBe(3);
    expect(routeGroupSize.get('c_d_2')).toBe(2);
    expect(routeGroupSize.get('e_f_1')).toBe(1);
    expect(groups.size).toBe(3);
  });
});
