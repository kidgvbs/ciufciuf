import { describe, it, expect } from 'vitest';
import { cityAnchors, countryAnchors } from '../map-utils';
import type { MapLayout } from '../map-utils';
import { flagShapes } from '../flags';

const city = (id: string, country = false) => ({ id, name: id, lat: 0, lon: 0, ...(country ? { country } : {}) });
const route = (id: string, from: string, to: string) => ({ id, from, to, len: 2, color: 'Gray', double: false });

type Layout = Pick<MapLayout, 'cities' | 'routes'>;

describe('countryAnchors', () => {
  const map = {
    cities: [city('basel'), city('zurich'), city('de', true), city('fr', true), city('it', true)],
    routes: [
      route('de_basel', 'de', 'basel'),
      route('de_zurich', 'de', 'zurich'),
      route('basel_zurich', 'basel', 'zurich'),
      route('fr_basel', 'basel', 'fr'),
    ],
  };

  it('layout diviso: un\'ancora per ogni valico, nell\'ordine delle rotte; senza rotte, la posizione del Paese', () => {
    const layout: Layout = {
      cities: {
        basel: { x: 100, y: 100 },
        zurich: { x: 300, y: 100 },
        de: { x: 200, y: 20 }, // posizione del nome, lontano dalle bandiere
        fr: { x: 0, y: 150 },
        it: { x: 250, y: 300 },
      },
      routes: {
        de_basel: [[60, 30], [100, 100]],
        de_zurich: [[340, 30], [300, 100]],
        basel_zurich: [[100, 100], [300, 100]],
        fr_basel: [[100, 100], [20, 140]],
      },
    };
    const anchors = countryAnchors(map, layout);
    expect(anchors.get('de')).toEqual([{ x: 60, y: 30 }, { x: 340, y: 30 }]);
    expect(anchors.get('fr')).toEqual([{ x: 20, y: 140 }]);
    expect(anchors.get('it')).toEqual([{ x: 250, y: 300 }]);
    expect([...anchors.keys()].sort()).toEqual(['de', 'fr', 'it']);
  });

  it('layout convergente: tutte le rotte finiscono nel punto del Paese, quindi una sola ancora', () => {
    const layout: Layout = {
      cities: {
        basel: { x: 100, y: 100 },
        zurich: { x: 300, y: 100 },
        de: { x: 200, y: 20 },
        fr: { x: 0, y: 150 },
        it: { x: 250, y: 300 },
      },
      routes: {
        de_basel: [[100, 100], [150, 60], [200, 20]], // con il «bend»: conta l'ultimo punto
        de_zurich: [[200, 20.2], [300, 100]], // a meno di 0.5 px: stessa bandiera
        basel_zurich: [[100, 100], [300, 100]],
        fr_basel: [[0, 150], [100, 100]],
      },
    };
    expect(countryAnchors(map, layout).get('de')).toEqual([{ x: 200, y: 20 }]);
    expect(countryAnchors(map, layout).get('fr')).toEqual([{ x: 0, y: 150 }]);
  });

  it('l\'ordine dei punti della polyline non cambia l\'esito', () => {
    const cities = { basel: { x: 100, y: 100 }, zurich: { x: 300, y: 100 }, de: { x: 0, y: 0 }, fr: { x: 0, y: 0 }, it: { x: 0, y: 0 } };
    const forward: Layout = {
      cities,
      routes: { de_basel: [[60, 30], [100, 100]], de_zurich: [[340, 30], [300, 100]], basel_zurich: [[100, 100], [300, 100]], fr_basel: [[100, 100], [20, 140]] },
    };
    const reversed: Layout = {
      cities,
      routes: { de_basel: [[100, 100], [60, 30]], de_zurich: [[300, 100], [340, 30]], basel_zurich: [[100, 100], [300, 100]], fr_basel: [[20, 140], [100, 100]] },
    };
    expect(countryAnchors(map, reversed)).toEqual(countryAnchors(map, forward));
  });

  it('una rotta senza polyline o senza posizione dell\'altro capo non dà ancora', () => {
    const layout: Layout = { cities: { basel: { x: 1, y: 1 }, de: { x: 5, y: 5 } }, routes: { de_zurich: [[9, 9], [1, 1]] } };
    const anchors = countryAnchors({ cities: map.cities, routes: [route('de_zurich', 'de', 'zurich'), route('de_basel', 'de', 'basel')] }, layout);
    expect(anchors.get('de')).toEqual([{ x: 5, y: 5 }]);
  });
});

describe('cityAnchors', () => {
  it('città normale: la sua posizione; Paese: le sue bandiere; senza posizione: assente', () => {
    const map = {
      cities: [city('a'), city('b'), city('c'), city('x', true)],
      routes: [route('r', 'a', 'x')],
    };
    const layout: Layout = {
      cities: { a: { x: 0, y: 0 }, b: { x: 5, y: 5 }, x: { x: 50, y: 50 } },
      routes: { r: [[0, 0], [30, 40]] },
    };
    const anchors = cityAnchors(map, layout);
    expect(anchors.get('a')).toEqual([{ x: 0, y: 0 }]);
    expect(anchors.get('b')).toEqual([{ x: 5, y: 5 }]);
    expect(anchors.has('c')).toBe(false);
    expect(anchors.get('x')).toEqual([{ x: 30, y: 40 }]);
  });
});

describe('flagShapes', () => {
  it('conosce i codici usati dalle mappe e rifiuta gli altri', () => {
    for (const code of ['CH', 'DE', 'AT', 'IT', 'FR', 'NL', 'DK', 'BY', 'CZ', 'LT', 'RU', 'SK', 'UA']) {
      expect(flagShapes(code)?.length, code).toBeGreaterThan(0);
    }
    expect(flagShapes(undefined)).toBeUndefined();
    expect(flagShapes('XX')).toBeUndefined();
    expect(flagShapes('constructor')).toBeUndefined();
  });
});
