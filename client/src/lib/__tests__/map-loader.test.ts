import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { clearMapCache, createMapTracker, isMapData, loadRulesetMap, type MapState } from '../map-loader';
import type { MapData } from '../map-utils';

const MAP = {
  cities: [{ id: 'a', name: 'A' }, { id: 'b', name: 'B' }],
  routes: [{ id: 'r1', from: 'a', to: 'b', len: 2, color: 'Red', double: false }],
  tickets: [{ id: 't1', from: 'a', to: 'b', points: 5, deck: 'regular' }],
};

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

let fetchMock: ReturnType<typeof vi.fn>;

beforeEach(() => {
  clearMapCache();
  fetchMock = vi.fn();
  vi.stubGlobal('fetch', fetchMock);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('loadRulesetMap', () => {
  it('richiede /api/rulesets/{id}/map e valida la risposta', async () => {
    fetchMock.mockResolvedValue(jsonResponse({ rulesetId: 'usa_v2', ...MAP }));
    const map = await loadRulesetMap('http://tv.local:8080', 'usa_v2');
    expect(map.tickets).toHaveLength(1);
    expect(fetchMock.mock.calls[0]?.[0]).toBe('http://tv.local:8080/api/rulesets/usa_v2/map');
  });

  it('memorizza la mappa e condivide le richieste concorrenti', async () => {
    fetchMock.mockImplementation(async () => jsonResponse(MAP));
    const [a, b] = await Promise.all([
      loadRulesetMap('http://h', 'usa_v2'),
      loadRulesetMap('http://h', 'usa_v2'),
    ]);
    await loadRulesetMap('http://h', 'usa_v2');
    expect(a).toBe(b);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    await loadRulesetMap('http://h', 'europe');
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('gli errori non restano in cache: 404 e risposta non valida', async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ error: 'NOT_FOUND', detail: null }, 404));
    await expect(loadRulesetMap('http://h', 'x')).rejects.toThrow('NOT_FOUND');
    fetchMock.mockResolvedValueOnce(jsonResponse({ cities: [] }));
    await expect(loadRulesetMap('http://h', 'x')).rejects.toThrow('non valida');
    fetchMock.mockResolvedValueOnce(jsonResponse(MAP));
    await expect(loadRulesetMap('http://h', 'x')).resolves.toBeDefined();
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });
});

describe('isMapData', () => {
  it('richiede città, rotte e biglietti con id e estremi testuali', () => {
    expect(isMapData(MAP)).toBe(true);
    expect(isMapData({ ...MAP, tickets: [{ id: 't', from: 'a' }] })).toBe(false);
    expect(isMapData({ ...MAP, cities: [{ name: 'x' }] })).toBe(false);
    expect(isMapData(null)).toBe(false);
  });
});

describe('createMapTracker', () => {
  const data = MAP as unknown as MapData;

  function setup() {
    const states: MapState[] = [];
    const tracker = createMapTracker((s) => states.push(s));
    return { states, tracker };
  }

  it('passa da loading a ready', async () => {
    const { states, tracker } = setup();
    tracker.track(async () => data);
    expect(states.map((s) => s.status)).toEqual(['loading']);
    await Promise.resolve();
    await Promise.resolve();
    expect(states.at(-1)).toEqual({ status: 'ready', data, error: '' });
  });

  it('errore e retry', async () => {
    const { states, tracker } = setup();
    const source = vi.fn<() => Promise<MapData>>()
      .mockRejectedValueOnce(new Error('Rete assente'))
      .mockResolvedValueOnce(data);
    tracker.track(source);
    await vi.waitFor(() => expect(states.at(-1)?.status).toBe('error'));
    expect(states.at(-1)?.error).toBe('Rete assente');
    tracker.retry();
    await vi.waitFor(() => expect(states.at(-1)?.status).toBe('ready'));
    expect(source).toHaveBeenCalledTimes(2);
  });

  it('una risposta superata da una richiesta più recente o da reset è ignorata', async () => {
    const { states, tracker } = setup();
    let resolveOld!: (m: MapData) => void;
    tracker.track(() => new Promise<MapData>((r) => { resolveOld = r; }));
    tracker.track(async () => ({ ...data, tickets: [] }));
    await vi.waitFor(() => expect(states.at(-1)?.status).toBe('ready'));
    resolveOld(data);
    await Promise.resolve();
    await Promise.resolve();
    expect(states.at(-1)?.data?.tickets).toHaveLength(0);

    let resolveLate!: (m: MapData) => void;
    tracker.track(() => new Promise<MapData>((r) => { resolveLate = r; }));
    tracker.reset();
    resolveLate(data);
    await Promise.resolve();
    await Promise.resolve();
    expect(states.at(-1)?.status).toBe('idle');
  });
});
