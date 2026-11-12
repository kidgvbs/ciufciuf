import { describe, it, expect, afterEach, vi } from 'vitest';
import {
  DEFAULT_RULESET_ID,
  FALLBACK_BONUS_LABEL,
  bonusLabel,
  defaultRulesetId,
  isRulesetInfo,
  loadRulesets,
  parseRulesetCatalog,
  rulesetBlurb,
  type RulesetInfo,
} from '../rulesets';

const usa: RulesetInfo = {
  id: 'usa_v2',
  name: 'USA',
  mapId: 'usa',
  cities: 36,
  routes: 100,
  tickets: 33,
  minPlayers: 2,
  maxPlayers: 5,
  bonusLabel: 'Longest Path',
  tunnels: false,
  ferries: false,
  stations: false,
};

const europe: RulesetInfo = {
  id: 'europe',
  name: 'Europe',
  mapId: 'europe',
  cities: 47,
  routes: 101,
  tickets: 46,
  minPlayers: 2,
  maxPlayers: 5,
  bonusLabel: 'European Express',
  tunnels: true,
  ferries: true,
  stations: true,
};

describe('isRulesetInfo', () => {
  it('accetta una voce completa', () => {
    expect(isRulesetInfo(usa)).toBe(true);
  });

  it('rifiuta campi mancanti o di tipo sbagliato', () => {
    const { name: _name, ...noName } = usa;
    expect(isRulesetInfo(noName)).toBe(false);
    expect(isRulesetInfo({ ...usa, id: '' })).toBe(false);
    expect(isRulesetInfo({ ...usa, cities: '36' })).toBe(false);
    expect(isRulesetInfo({ ...usa, routes: -1 })).toBe(false);
    expect(isRulesetInfo({ ...usa, tickets: 1.5 })).toBe(false);
    expect(isRulesetInfo({ ...usa, tunnels: 'no' })).toBe(false);
    expect(isRulesetInfo({ ...usa, bonusLabel: 7 })).toBe(false);
    expect(isRulesetInfo(null)).toBe(false);
    expect(isRulesetInfo('usa_v2')).toBe(false);
  });

  it('rifiuta un intervallo di giocatori incoerente', () => {
    expect(isRulesetInfo({ ...usa, minPlayers: 0 })).toBe(false);
    expect(isRulesetInfo({ ...usa, minPlayers: 6, maxPlayers: 5 })).toBe(false);
  });
});

describe('parseRulesetCatalog', () => {
  it('restituisce le voci nell ordine del server', () => {
    expect(parseRulesetCatalog({ rulesets: [europe, usa] })).toEqual([europe, usa]);
  });

  it('accetta un catalogo vuoto', () => {
    expect(parseRulesetCatalog({ rulesets: [] })).toEqual([]);
  });

  it.each([
    ['null', null],
    ['senza rulesets', {}],
    ['rulesets non array', { rulesets: 'usa_v2' }],
    ['voce non valida', { rulesets: [usa, { id: 'x' }] }],
    ['id duplicati', { rulesets: [usa, usa] }],
  ])('rifiuta una risposta %s con «Risposta inattesa dal server»', (_label, body) => {
    expect(() => parseRulesetCatalog(body)).toThrow('Risposta inattesa dal server');
  });
});

describe('loadRulesets', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  function stubFetch(response: Response) {
    const fetchMock = vi.fn().mockResolvedValue(response);
    vi.stubGlobal('fetch', fetchMock);
    return fetchMock;
  }

  function json(body: unknown, status = 200): Response {
    return new Response(JSON.stringify(body), {
      status,
      headers: { 'Content-Type': 'application/json' },
    });
  }

  it('legge GET /api/rulesets e valida la forma', async () => {
    const fetchMock = stubFetch(json({ rulesets: [usa, europe] }));
    await expect(loadRulesets('http://tv.local:8080/')).resolves.toEqual([usa, europe]);
    expect(fetchMock.mock.calls[0]![0]).toBe('http://tv.local:8080/api/rulesets');
    expect(fetchMock.mock.calls[0]![1]).toMatchObject({ method: 'GET' });
  });

  it('una forma non valida dà «Risposta inattesa dal server»', async () => {
    stubFetch(json({ rulesets: [{ id: 'usa_v2' }] }));
    await expect(loadRulesets('http://tv.local')).rejects.toThrow('Risposta inattesa dal server');
  });

  it('un corpo non JSON dà «Risposta inattesa dal server»', async () => {
    stubFetch(new Response('<html></html>', { status: 200, headers: { 'Content-Type': 'text/html' } }));
    await expect(loadRulesets('http://tv.local')).rejects.toThrow('Risposta inattesa dal server');
  });

  it('propaga l errore del server', async () => {
    stubFetch(json({ error: 'INTERNAL', detail: 'boom' }, 500));
    await expect(loadRulesets('http://tv.local')).rejects.toThrow('boom');
  });
});

describe('rulesetBlurb', () => {
  it('senza caratteristiche speciali elenca solo città e rotte', () => {
    expect(rulesetBlurb(usa)).toBe('36 città, 100 rotte');
  });

  it('elenca tunnel, traghetti e stazioni', () => {
    expect(rulesetBlurb(europe)).toBe('47 città, 101 rotte, con tunnel, traghetti e stazioni');
  });

  it('una sola caratteristica', () => {
    expect(rulesetBlurb({ ...usa, tunnels: true })).toBe('36 città, 100 rotte, con tunnel');
  });

  it('due caratteristiche', () => {
    expect(rulesetBlurb({ ...usa, ferries: true, stations: true })).toBe(
      '36 città, 100 rotte, con traghetti e stazioni',
    );
  });
});

describe('bonusLabel', () => {
  it('prende il nome del bonus dal catalogo', () => {
    expect(bonusLabel([usa, europe], 'europe')).toBe('European Express');
    expect(bonusLabel([usa, europe], 'usa_v2')).toBe('Longest Path');
  });

  it('ripiega su "Longest Path" se il ruleset non c è, è vuoto o il catalogo è vuoto', () => {
    expect(bonusLabel([usa, europe], 'sconosciuto')).toBe(FALLBACK_BONUS_LABEL);
    expect(bonusLabel([usa, europe], '')).toBe(FALLBACK_BONUS_LABEL);
    expect(bonusLabel([usa, europe], null)).toBe(FALLBACK_BONUS_LABEL);
    expect(bonusLabel([usa, europe], undefined)).toBe('Longest Path');
    expect(bonusLabel([], 'europe')).toBe('Longest Path');
  });
});

describe('defaultRulesetId', () => {
  it('preferisce il default se presente (anche non per primo)', () => {
    expect(defaultRulesetId([europe, usa])).toBe(DEFAULT_RULESET_ID);
  });

  it('altrimenti il primo del catalogo', () => {
    expect(defaultRulesetId([europe])).toBe('europe');
  });

  it('catalogo vuoto → null', () => {
    expect(defaultRulesetId([])).toBeNull();
  });
});
