import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  clearMapCache,
  createMapTracker,
  isMapData,
  isMapLayout,
  loadRulesetMap,
  type MapState,
  type RulesetMap,
} from '../map-loader';
import { MAP_FETCH_TIMEOUT_MS } from '../lobby';
import type { MapData, MapLayout } from '../map-utils';

const MAP = {
  cities: [{ id: 'a', name: 'A' }, { id: 'b', name: 'B' }],
  routes: [{ id: 'r1', from: 'a', to: 'b', len: 2, color: 'Red', double: false }],
  tickets: [{ id: 't1', from: 'a', to: 'b', points: 5, deck: 'regular' }],
};

const LAYOUT = {
  unit: 34,
  width: 100,
  height: 60,
  cities: { a: { x: 10, y: 20 }, b: { x: 80, y: 40 } },
  labels: {
    a: { dx: 0, dy: -12, anchor: 'middle' },
    b: { dx: 0, dy: 20, anchor: 'start' },
  },
  routes: { r1: [[10, 20], [45, 25], [80, 40]] },
  font: { family: 'Barlow Semi Condensed', weight: 600, size: 15.5 },
  doubleGap: 5,
};

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

interface Reply {
  body: unknown;
  status?: number;
}

let fetchMock: ReturnType<typeof vi.fn>;

/** Il server di prova: risponde a `/map` e `/layout` (di default con `MAP` e `LAYOUT`). */
function serve(replies: { map?: Reply; layout?: Reply } = {}): void {
  const map = replies.map ?? { body: MAP };
  const layout = replies.layout ?? { body: LAYOUT };
  fetchMock.mockImplementation(async (url: string) => {
    const reply = url.endsWith('/layout') ? layout : map;
    return jsonResponse(reply.body, reply.status);
  });
}

function requestedUrls(): string[] {
  return fetchMock.mock.calls.map((call) => String(call[0]));
}

beforeEach(() => {
  clearMapCache();
  fetchMock = vi.fn();
  vi.stubGlobal('fetch', fetchMock);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('loadRulesetMap', () => {
  it('richiede in parallelo /map e /layout del ruleset e restituisce entrambi', async () => {
    serve({ map: { body: { rulesetId: 'usa_v2', ...MAP } }, layout: { body: { rulesetId: 'usa_v2', mapId: 'usa', ...LAYOUT } } });
    const pending = loadRulesetMap('http://tv.local:8080', 'usa_v2');
    expect(requestedUrls().sort()).toEqual([
      'http://tv.local:8080/api/rulesets/usa_v2/layout',
      'http://tv.local:8080/api/rulesets/usa_v2/map',
    ]);
    const { map, layout } = await pending;
    expect(map.tickets).toHaveLength(1);
    expect(layout?.routes['r1']).toEqual([[10, 20], [45, 25], [80, 40]]);
    expect(layout?.width).toBe(100);
  });

  it('memorizza mappa e layout e condivide le richieste concorrenti', async () => {
    serve();
    const [a, b] = await Promise.all([
      loadRulesetMap('http://h', 'usa_v2'),
      loadRulesetMap('http://h', 'usa_v2'),
    ]);
    await loadRulesetMap('http://h', 'usa_v2');
    expect(a).toBe(b);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    await loadRulesetMap('http://h', 'europe');
    expect(fetchMock).toHaveBeenCalledTimes(4);
  });

  it('gli errori non restano in cache: 404 e risposta non valida', async () => {
    serve({ map: { body: { error: 'NOT_FOUND', detail: null }, status: 404 } });
    await expect(loadRulesetMap('http://h', 'x')).rejects.toThrow('NOT_FOUND');
    serve({ map: { body: { cities: [] } } });
    await expect(loadRulesetMap('http://h', 'x')).rejects.toThrow('non valida');
    serve();
    await expect(loadRulesetMap('http://h', 'x')).resolves.toBeDefined();
    expect(fetchMock).toHaveBeenCalledTimes(6);
  });

  it('un layout che non copre la mappa dà un errore leggibile e non resta in cache', async () => {
    const withoutRoute = { ...LAYOUT, routes: {} };
    serve({ layout: { body: withoutRoute } });
    await expect(loadRulesetMap('http://h', 'usa_v2')).rejects.toThrow('Layout della mappa incompleto');

    const withoutCity = { ...LAYOUT, cities: { a: LAYOUT.cities.a } };
    serve({ layout: { body: withoutCity } });
    await expect(loadRulesetMap('http://h', 'usa_v2')).rejects.toThrow('Layout della mappa incompleto');

    const withoutLabel = { ...LAYOUT, labels: { a: LAYOUT.labels.a } };
    serve({ layout: { body: withoutLabel } });
    await expect(loadRulesetMap('http://h', 'usa_v2')).rejects.toThrow('Layout della mappa incompleto');

    serve();
    await expect(loadRulesetMap('http://h', 'usa_v2')).resolves.toBeDefined();
  });

  it('un layout malformato è rifiutato', async () => {
    serve({ layout: { body: { ...LAYOUT, width: 'largo' } } });
    await expect(loadRulesetMap('http://h', 'usa_v2')).rejects.toThrow('Layout della mappa non valido');
  });
});

describe('isMapData', () => {
  it('richiede città, rotte e biglietti con id e estremi testuali', () => {
    expect(isMapData(MAP)).toBe(true);
    expect(isMapData({ ...MAP, tickets: [{ id: 't', from: 'a' }] })).toBe(false);
    expect(isMapData({ ...MAP, cities: [{ name: 'x' }] })).toBe(false);
    expect(isMapData(null)).toBe(false);
  });

  const withRoute = (patch: Record<string, unknown>) => ({
    ...MAP,
    routes: [{ ...MAP.routes[0], ...patch }],
  });

  it('accetta tunnel e ferries opzionali (assenti sulla mappa USA)', () => {
    expect(isMapData(withRoute({ tunnel: true, ferries: 0 }))).toBe(true);
    expect(isMapData(withRoute({ tunnel: false, ferries: 2 }))).toBe(true);
    expect(isMapData(withRoute({ ferries: 2 }))).toBe(true);
  });

  it('rifiuta tunnel non booleano e ferries non interi, negativi o oltre la lunghezza', () => {
    expect(isMapData(withRoute({ tunnel: 'sì' }))).toBe(false);
    for (const ferries of [-1, 1.5, 3, '1', null]) {
      expect(isMapData(withRoute({ ferries })), `ferries=${String(ferries)}`).toBe(false);
    }
  });
});

describe('isMapLayout', () => {
  it('accetta il layout completo e ignora i campi opzionali assenti', () => {
    expect(isMapLayout(LAYOUT)).toBe(true);
    const { font: _font, doubleGap: _gap, unit: _unit, ...minimal } = LAYOUT;
    expect(isMapLayout(minimal)).toBe(true);
  });

  it('rifiuta dimensioni, posizioni, etichette, rotte e font malformati', () => {
    expect(isMapLayout(null)).toBe(false);
    expect(isMapLayout({ ...LAYOUT, width: 0 })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, height: null })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, cities: { a: { x: 1 } } })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, cities: [] })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, labels: { a: { dx: 0, dy: 0, anchor: 'left' } } })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, routes: { r1: [[1, 2]] } })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, routes: { r1: [[1, 2], [2, 3], [3, 4], [4, 5]] } })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, routes: { r1: [[1, 2], [3]] } })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, routes: { r1: [[1, 2], [3, 'x']] } })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, font: { family: 'x' } })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, doubleGap: 'cinque' })).toBe(false);
  });

  it('layout_with_huge_values_is_rejected', () => {
    const huge = 100_001;
    expect(isMapLayout({ ...LAYOUT, width: huge })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, height: 1e308 })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, width: 100_000, height: 100_000 })).toBe(true);
    expect(isMapLayout({ ...LAYOUT, cities: { a: { x: huge, y: 0 } } })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, cities: { a: { x: 0, y: -huge } } })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, cities: { a: { x: -100_000, y: 100_000 } } })).toBe(true);
    expect(isMapLayout({ ...LAYOUT, cities: { a: { x: Infinity, y: 0 } } })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, labels: { a: { dx: huge, dy: 0, anchor: 'middle' } } })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, labels: { a: { dx: 0, dy: -huge, anchor: 'middle' } } })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, routes: { r1: [[0, 0], [huge, 0]] } })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, routes: { r1: [[0, 0], [0, 1], [0, -huge]] } })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, unit: 1001 })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, unit: -1 })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, doubleGap: 1001 })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, doubleGap: -1 })).toBe(false);
    expect(isMapLayout({ ...LAYOUT, unit: 0, doubleGap: 1000 })).toBe(true);
  });

  it('un layout con valori enormi dal server dà «Layout della mappa non valido»', async () => {
    serve({ layout: { body: { ...LAYOUT, width: 1e9 } } });
    await expect(loadRulesetMap('http://h', 'usa_v2')).rejects.toThrow('Layout della mappa non valido');
  });
});

describe('createMapTracker', () => {
  const bundle: RulesetMap = { map: MAP as unknown as MapData, layout: LAYOUT as unknown as MapLayout };
  const data = bundle.map;

  function setup() {
    const states: MapState[] = [];
    const tracker = createMapTracker((s) => states.push(s));
    return { states, tracker };
  }

  it('passa da loading a ready', async () => {
    const { states, tracker } = setup();
    tracker.track(async () => bundle);
    expect(states.map((s) => s.status)).toEqual(['loading']);
    await Promise.resolve();
    await Promise.resolve();
    expect(states.at(-1)).toEqual({ status: 'ready', data, layout: bundle.layout, error: '' });
  });

  it('errore e retry', async () => {
    const { states, tracker } = setup();
    const source = vi.fn<() => Promise<RulesetMap>>()
      .mockRejectedValueOnce(new Error('Rete assente'))
      .mockResolvedValueOnce(bundle);
    tracker.track(source);
    await vi.waitFor(() => expect(states.at(-1)?.status).toBe('error'));
    expect(states.at(-1)?.error).toBe('Rete assente');
    tracker.retry();
    await vi.waitFor(() => expect(states.at(-1)?.status).toBe('ready'));
    expect(source).toHaveBeenCalledTimes(2);
  });

  it('layout assente (404 LAYOUT_NOT_FOUND) lascia la mappa pronta con layout null', async () => {
    const { states, tracker } = setup();
    serve({
      layout: {
        body: { error: 'LAYOUT_NOT_FOUND', detail: 'Layout della mappa non trovato.' },
        status: 404,
      },
    });
    tracker.track(() => loadRulesetMap('http://h', 'usa_v2'));
    await vi.waitFor(() => expect(states.at(-1)?.status).toBe('ready'));
    const ready = states.at(-1);
    expect(ready?.data?.cities).toHaveLength(2);
    expect(ready?.layout).toBeNull();

    // Il risultato con layout null resta in cache.
    await loadRulesetMap('http://h', 'usa_v2');
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('errore del server sul layout porta allo stato di errore e Riprova lo recupera', async () => {
    const { states, tracker } = setup();
    serve({
      layout: {
        body: { error: 'INTERNAL_ERROR', detail: 'Errore interno del server.' },
        status: 500,
      },
    });
    tracker.track(() => loadRulesetMap('http://h', 'usa_v2'));
    await vi.waitFor(() => expect(states.at(-1)?.status).toBe('error'));
    expect(states.at(-1)).toEqual({
      status: 'error',
      data: null,
      layout: null,
      error: 'Errore interno del server.',
    });

    serve();
    tracker.retry();
    await vi.waitFor(() => expect(states.at(-1)?.status).toBe('ready'));
    expect(states.at(-1)?.layout?.width).toBe(100);
  });

  it('una risposta superata da una richiesta più recente o da reset è ignorata', async () => {
    const { states, tracker } = setup();
    let resolveOld!: (m: RulesetMap) => void;
    tracker.track(() => new Promise<RulesetMap>((r) => { resolveOld = r; }));
    tracker.track(async () => ({ ...bundle, map: { ...data, tickets: [] } }));
    await vi.waitFor(() => expect(states.at(-1)?.status).toBe('ready'));
    resolveOld(bundle);
    await Promise.resolve();
    await Promise.resolve();
    expect(states.at(-1)?.data?.tickets).toHaveLength(0);

    let resolveLate!: (m: RulesetMap) => void;
    tracker.track(() => new Promise<RulesetMap>((r) => { resolveLate = r; }));
    tracker.reset();
    resolveLate(bundle);
    await Promise.resolve();
    await Promise.resolve();
    expect(states.at(-1)?.status).toBe('idle');
  });
});

describe('robustezza del caricamento', () => {
  const bundle: RulesetMap = { map: MAP as unknown as MapData, layout: LAYOUT as unknown as MapLayout };

  it('map_fetch_times_out_into_error', async () => {
    vi.useFakeTimers();
    try {
      fetchMock.mockImplementation(
        (_url: string, init: RequestInit) =>
          new Promise((_resolve, reject) => {
            init.signal?.addEventListener('abort', () =>
              reject(new DOMException('The operation was aborted.', 'AbortError')),
            );
          }),
      );
      const states: MapState[] = [];
      const tracker = createMapTracker((s) => states.push(s));
      tracker.track(() => loadRulesetMap('http://h', 'usa_v2'));
      await vi.advanceTimersByTimeAsync(MAP_FETCH_TIMEOUT_MS - 1);
      expect(states.at(-1)?.status).toBe('loading');
      await vi.advanceTimersByTimeAsync(1);
      expect(states.at(-1)).toEqual({
        status: 'error',
        data: null,
        layout: null,
        error: 'Timeout caricamento mappa',
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it('layout_fetch_times_out_into_error', async () => {
    vi.useFakeTimers();
    try {
      fetchMock.mockImplementation((url: string, init: RequestInit) =>
        url.endsWith('/layout')
          ? new Promise((_resolve, reject) => {
              init.signal?.addEventListener('abort', () =>
                reject(new DOMException('The operation was aborted.', 'AbortError')),
              );
            })
          : Promise.resolve(jsonResponse(MAP)),
      );
      const pending = loadRulesetMap('http://h', 'usa_v2');
      const outcome = pending.then(
        () => 'ok',
        (e: Error) => e.message,
      );
      await vi.advanceTimersByTimeAsync(MAP_FETCH_TIMEOUT_MS);
      await expect(outcome).resolves.toBe('Timeout caricamento mappa');
    } finally {
      vi.useRealTimers();
    }
  });

  it('tracker_times_out_when_source_never_resolves', async () => {
    vi.useFakeTimers();
    try {
      const states: MapState[] = [];
      const tracker = createMapTracker((s) => states.push(s));
      let resolveLate!: (m: RulesetMap) => void;
      tracker.track(() => new Promise<RulesetMap>((r) => { resolveLate = r; }));
      await vi.advanceTimersByTimeAsync(MAP_FETCH_TIMEOUT_MS - 1);
      expect(states.at(-1)?.status).toBe('loading');
      await vi.advanceTimersByTimeAsync(1);
      expect(states.at(-1)).toEqual({
        status: 'error',
        data: null,
        layout: null,
        error: 'Timeout caricamento mappa',
      });
      resolveLate(bundle);
      await vi.advanceTimersByTimeAsync(0);
      expect(states.at(-1)?.status).toBe('error');

      tracker.retry();
      await vi.advanceTimersByTimeAsync(0);
      expect(states.at(-1)?.status).toBe('loading');
      tracker.reset();
      await vi.advanceTimersByTimeAsync(MAP_FETCH_TIMEOUT_MS);
      expect(states.at(-1)?.status).toBe('idle');
    } finally {
      vi.useRealTimers();
    }
  });

  it('non_json_response_gives_readable_error', async () => {
    fetchMock.mockResolvedValue(
      new Response('<html></html>', { status: 200, headers: { 'Content-Type': 'text/html' } }),
    );
    await expect(loadRulesetMap('http://h', 'usa_v2')).rejects.toThrow('Risposta inattesa dal server');
  });

  it('route_with_invalid_len_is_rejected', async () => {
    const withRoute = (patch: Record<string, unknown>) => ({
      ...MAP,
      routes: [{ ...MAP.routes[0], ...patch }],
    });
    for (const len of [0, 13, 2.5, '2', null]) {
      expect(isMapData(withRoute({ len })), `len=${String(len)}`).toBe(false);
    }
    expect(isMapData(withRoute({ len: 1 }))).toBe(true);
    expect(isMapData(withRoute({ len: 12 }))).toBe(true);
    expect(isMapData(withRoute({ double: 'no' }))).toBe(false);
    expect(isMapData(withRoute({ color: 3 }))).toBe(false);
    serve({ map: { body: withRoute({ len: 0 }) } });
    await expect(loadRulesetMap('http://h', 'usa_v2')).rejects.toThrow('non valida');
  });

  it('home_remount_reuses_cached_map', async () => {
    serve();
    const tracker = createMapTracker(() => {});
    await loadRulesetMap('http://h', 'usa_v2');
    await loadRulesetMap('http://h', 'usa_v2');
    expect(fetchMock).toHaveBeenCalledTimes(2);
    tracker.reset();
    await loadRulesetMap('http://h', 'usa_v2');
    expect(fetchMock).toHaveBeenCalledTimes(2);
    tracker.track(async () => bundle);
    await loadRulesetMap('http://h', 'usa_v2');
    expect(fetchMock).toHaveBeenCalledTimes(4);
  });
});
