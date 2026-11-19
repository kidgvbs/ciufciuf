import { describe, it, expect, afterEach, vi } from 'vitest';
import realCatalog from './rulesets-catalog.fixture.json';
import {
  DEFAULT_RULESET_ID,
  FALLBACK_GLOBETROTTER_LABEL,
  FALLBACK_LONGEST_PATH_LABEL,
  FALLBACK_MANDALA_LABEL,
  bigCities,
  defaultRulesetId,
  globetrotterLabel,
  groupByMap,
  hasGlobetrotter,
  hasLongestPath,
  hasMandala,
  isRulesetInfo,
  loadRulesets,
  longestPathLabel,
  mandalaLabel,
  modeBlurb,
  parseRulesetCatalog,
  rulesetBlurb,
  rulesetTitle,
  rulesetTitleById,
  specialRules,
  type RulesetInfo,
} from '../rulesets';

const usa: RulesetInfo = {
  id: 'usa_v2',
  name: 'USA',
  mapId: 'usa',
  mode: 'Standard',
  modeOrder: 0,
  cities: 36,
  routes: 100,
  tickets: 33,
  minPlayers: 2,
  maxPlayers: 5,
  bonuses: [{ type: 'longest_path', label: 'Longest Path', points: 10 }],
  bigCities: [],
  initialDealt: 4,
  initialKeepMin: 2,
  drawCount: 3,
  drawKeepMin: 1,
  tunnels: false,
  ferries: false,
  stations: false,
};

const usa1910: RulesetInfo = {
  ...usa,
  id: 'usa_1910',
  mode: '1910',
  modeOrder: 1,
  bonuses: [{ type: 'globetrotter', label: 'Globetrotter', points: 15 }],
};

const usaBig: RulesetInfo = {
  ...usa,
  id: 'usa_big_cities',
  mode: 'Grandi Città',
  modeOrder: 2,
  bonuses: [],
  bigCities: ['seattle', 'denver'],
  drawCount: 4,
};

const usaMega: RulesetInfo = {
  ...usa,
  id: 'usa_mega',
  mode: 'Mega',
  modeOrder: 3,
  initialDealt: 5,
  initialKeepMin: 3,
  drawCount: 4,
  bonuses: [
    { type: 'longest_path', label: 'Longest Path', points: 10 },
    { type: 'globetrotter', label: 'Globetrotter', points: 15 },
  ],
};

const europe: RulesetInfo = {
  id: 'europe',
  name: 'Europe',
  mapId: 'europe',
  mode: 'Standard',
  modeOrder: 0,
  cities: 47,
  routes: 101,
  tickets: 46,
  minPlayers: 2,
  maxPlayers: 5,
  bonuses: [{ type: 'longest_path', label: 'European Express', points: 10 }],
  bigCities: [],
  initialDealt: 4,
  initialKeepMin: 2,
  drawCount: 3,
  drawKeepMin: 1,
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
    expect(isRulesetInfo({ ...usa, mode: '' })).toBe(false);
    expect(isRulesetInfo({ ...usa, modeOrder: -1 })).toBe(false);
    expect(isRulesetInfo({ ...usa, bonuses: 'longest_path' })).toBe(false);
    expect(isRulesetInfo({ ...usa, bonuses: [{ type: 'longest_path', label: 'x' }] })).toBe(false);
    expect(isRulesetInfo({ ...usa, bigCities: 'seattle' })).toBe(false);
    expect(isRulesetInfo({ ...usa, bigCities: [''] })).toBe(false);
    expect(isRulesetInfo({ ...usa, initialDealt: 4.5 })).toBe(false);
    expect(isRulesetInfo({ ...usa, drawKeepMin: -1 })).toBe(false);
    expect(isRulesetInfo({ ...usa, bonusLabel: 'Longest Path', bonuses: undefined })).toBe(false);
    expect(isRulesetInfo(null)).toBe(false);
    expect(isRulesetInfo('usa_v2')).toBe(false);
  });

  it('accetta bonus vuoti e tipi di bonus sconosciuti', () => {
    expect(isRulesetInfo(usaBig)).toBe(true);
    expect(isRulesetInfo({ ...usa, bonuses: [{ type: 'futuro', label: 'Boh', points: 3 }] })).toBe(true);
  });

  it('rifiuta un intervallo di giocatori incoerente', () => {
    expect(isRulesetInfo({ ...usa, minPlayers: 0 })).toBe(false);
    expect(isRulesetInfo({ ...usa, minPlayers: 6, maxPlayers: 5 })).toBe(false);
  });

  it('specialRules è opzionale (server vecchi) e, se presente, è un array di testi non vuoti', () => {
    expect(isRulesetInfo({ ...usa, specialRules: [] })).toBe(true);
    expect(isRulesetInfo({ ...usa, specialRules: ['35 treni a testa.'] })).toBe(true);
    expect(isRulesetInfo({ ...usa, specialRules: 'regola' })).toBe(false);
    expect(isRulesetInfo({ ...usa, specialRules: [''] })).toBe(false);
    expect(isRulesetInfo({ ...usa, specialRules: [3] })).toBe(false);
  });
});

describe('specialRules', () => {
  const poland: RulesetInfo = { ...usa, id: 'poland', specialRules: ['35 treni a testa (non 45).'] };

  it('restituisce i testi del ruleset, vuoto se assenti o ruleset ignoto', () => {
    const catalog = [usa, poland];
    expect(specialRules(catalog, 'poland')).toEqual(['35 treni a testa (non 45).']);
    expect(specialRules(catalog, 'usa_v2')).toEqual([]);
    expect(specialRules(catalog, 'sconosciuto')).toEqual([]);
    expect(specialRules(catalog, null)).toEqual([]);
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

describe('risposta reale di GET /api/rulesets (fixture catturata dal server)', () => {
  const catalog = parseRulesetCatalog(realCatalog);

  it('viene accettata e il suo ordine è quello del server', () => {
    expect(catalog.map((r) => r.id)).toEqual([
      'europe',
      'europe_expanded',
      'europe_1912',
      'europe_big_cities',
      'europe_mega',
      'usa_v2',
      'usa_1910',
      'usa_big_cities',
      'usa_mega',
    ]);
  });

  it('i ruleset USA si raggruppano in una mappa con le modalità in ordine', () => {
    const groups = groupByMap(catalog);
    expect(groups.map((g) => g.mapId)).toEqual(['europe', 'usa']);
    expect(groups[1]!.rulesets.map((r) => rulesetTitle(r))).toEqual([
      'USA',
      'USA · 1910',
      'USA · Grandi Città',
      'USA · Mega',
    ]);
  });

  it('bonus e grandi città derivano dal catalogo', () => {
    expect(hasLongestPath(catalog, 'usa_1910')).toBe(false);
    expect(hasGlobetrotter(catalog, 'usa_1910')).toBe(true);
    expect(hasLongestPath(catalog, 'usa_mega') && hasGlobetrotter(catalog, 'usa_mega')).toBe(true);
    expect(longestPathLabel(catalog, 'europe')).toBe('European Express');
    expect(bigCities(catalog, 'usa_big_cities')).toHaveLength(7);
    expect(bigCities(catalog, 'europe_big_cities')).toHaveLength(9);
    expect(modeBlurb(catalog.find((r) => r.id === 'europe_mega')!)).toBe(
      '101 biglietti in gioco · ricevi 7, tieni 3 · pesca 3, tieni 1 · European Express +10',
    );
    expect(modeBlurb(catalog.find((r) => r.id === 'usa_mega')!)).toBe(
      '69 biglietti in gioco · ricevi 5, tieni 3 · pesca 4, tieni 1 · Longest Path +10 · Globetrotter +15',
    );
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

describe('etichette dei bonus', () => {
  const catalog = [usa, usa1910, europe, usaMega];

  it('longestPathLabel prende il nome dal catalogo', () => {
    expect(longestPathLabel(catalog, 'europe')).toBe('European Express');
    expect(longestPathLabel(catalog, 'usa_v2')).toBe('Longest Path');
  });

  it('longestPathLabel ripiega su "Longest Path" se ruleset ignoto, vuoto, null o senza il bonus', () => {
    expect(longestPathLabel(catalog, 'sconosciuto')).toBe(FALLBACK_LONGEST_PATH_LABEL);
    expect(longestPathLabel(catalog, '')).toBe(FALLBACK_LONGEST_PATH_LABEL);
    expect(longestPathLabel(catalog, null)).toBe(FALLBACK_LONGEST_PATH_LABEL);
    expect(longestPathLabel(catalog, undefined)).toBe('Longest Path');
    expect(longestPathLabel([], 'europe')).toBe('Longest Path');
    expect(longestPathLabel(catalog, 'usa_1910')).toBe('Longest Path');
  });

  it('globetrotterLabel con fallback "Globetrotter"', () => {
    const custom = { ...usa1910, bonuses: [{ type: 'globetrotter', label: 'Giramondo', points: 15 }] };
    expect(globetrotterLabel([custom], 'usa_1910')).toBe('Giramondo');
    expect(globetrotterLabel(catalog, 'usa_1910')).toBe('Globetrotter');
    expect(globetrotterLabel(catalog, 'usa_v2')).toBe(FALLBACK_GLOBETROTTER_LABEL);
    expect(globetrotterLabel([], null)).toBe('Globetrotter');
  });

  it('hasLongestPath / hasGlobetrotter seguono i bonus del ruleset', () => {
    expect(hasLongestPath(catalog, 'usa_v2')).toBe(true);
    expect(hasLongestPath(catalog, 'usa_1910')).toBe(false);
    expect(hasLongestPath([usaBig], 'usa_big_cities')).toBe(false);
    expect(hasLongestPath(catalog, 'usa_mega')).toBe(true);
    expect(hasGlobetrotter(catalog, 'usa_1910')).toBe(true);
    expect(hasGlobetrotter(catalog, 'usa_v2')).toBe(false);
    expect(hasGlobetrotter(catalog, 'usa_mega')).toBe(true);
  });

  it('con ruleset ignoto: percorso più lungo sì (storico), Globetrotter no', () => {
    expect(hasLongestPath([], 'usa_v2')).toBe(true);
    expect(hasLongestPath(catalog, null)).toBe(true);
    expect(hasGlobetrotter([], 'usa_v2')).toBe(false);
    expect(hasGlobetrotter(catalog, undefined)).toBe(false);
  });
});

const mandalaBonus = {
  type: 'mandala',
  label: 'Grand Tour',
  points: 40,
  table: [5, 10, 20, 30, 40],
};
const india: RulesetInfo = {
  ...usa,
  id: 'india',
  mapId: 'india',
  bonuses: [{ type: 'longest_path', label: 'Indian Express', points: 10 }, mandalaBonus],
};

describe('bonus Mandala', () => {
  it('accetta la tabella opzionale e rifiuta valori non interi', () => {
    expect(isRulesetInfo(india)).toBe(true);
    expect(isRulesetInfo({ ...india, bonuses: [{ ...mandalaBonus, table: [5, 1.5] }] })).toBe(false);
    expect(isRulesetInfo({ ...india, bonuses: [{ ...mandalaBonus, table: 'x' }] })).toBe(false);
  });

  it('mandalaLabel con fallback "Grand Tour"', () => {
    expect(mandalaLabel([india], 'india')).toBe('Grand Tour');
    const custom = { ...india, bonuses: [{ ...mandalaBonus, label: 'Mandala' }] };
    expect(mandalaLabel([custom], 'india')).toBe('Mandala');
    expect(mandalaLabel([usa], 'usa_v2')).toBe(FALLBACK_MANDALA_LABEL);
    expect(mandalaLabel([], null)).toBe('Grand Tour');
  });

  it('hasMandala segue i bonus del ruleset (ignoto → false)', () => {
    expect(hasMandala([india, usa], 'india')).toBe(true);
    expect(hasMandala([india, usa], 'usa_v2')).toBe(false);
    expect(hasMandala([], 'india')).toBe(false);
    expect(hasMandala([india], undefined)).toBe(false);
  });

  it('modeBlurb mostra il massimo del bonus', () => {
    expect(modeBlurb(india)).toBe(
      '33 biglietti in gioco · ricevi 4, tieni 2 · pesca 3, tieni 1 · Indian Express +10 · Grand Tour fino a +40',
    );
  });
});

describe('bigCities', () => {
  it('restituisce le grandi città del ruleset', () => {
    expect(bigCities([usa, usaBig], 'usa_big_cities')).toEqual(['seattle', 'denver']);
  });

  it('vuoto se il ruleset non ne ha, è ignoto o assente', () => {
    expect(bigCities([usa, usaBig], 'usa_v2')).toEqual([]);
    expect(bigCities([usa, usaBig], 'boh')).toEqual([]);
    expect(bigCities([usa, usaBig], null)).toEqual([]);
  });
});

describe('rulesetTitle', () => {
  it('«Nome · Modalità», solo il nome per Standard', () => {
    expect(rulesetTitle(usa)).toBe('USA');
    expect(rulesetTitle(usa1910)).toBe('USA · 1910');
    expect(rulesetTitle(usaBig)).toBe('USA · Grandi Città');
  });

  it('rulesetTitleById: dal catalogo, null se ignoto', () => {
    expect(rulesetTitleById([usa, usa1910], 'usa_1910')).toBe('USA · 1910');
    expect(rulesetTitleById([usa, usa1910], 'usa_v2')).toBe('USA');
    expect(rulesetTitleById([usa], 'x')).toBeNull();
    expect(rulesetTitleById([usa], '')).toBeNull();
    expect(rulesetTitleById([usa], undefined)).toBeNull();
  });
});

describe('groupByMap', () => {
  it('raggruppa per mappa nell ordine di prima comparsa e ordina le modalità per modeOrder', () => {
    const groups = groupByMap([usaMega, europe, usa1910, usa, usaBig]);
    expect(groups.map((g) => g.mapId)).toEqual(['usa', 'europe']);
    expect(groups[0]!.rulesets.map((r) => r.id)).toEqual(['usa_v2', 'usa_1910', 'usa_big_cities', 'usa_mega']);
    expect(groups[1]!.rulesets.map((r) => r.id)).toEqual(['europe']);
  });

  it('non modifica il catalogo e a pari modeOrder resta l ordine del catalogo', () => {
    const catalog = [usa1910, usa, { ...usaBig, modeOrder: 1 }];
    const before = catalog.map((r) => r.id);
    expect(groupByMap(catalog)[0]!.rulesets.map((r) => r.id)).toEqual(['usa_v2', 'usa_1910', 'usa_big_cities']);
    expect(catalog.map((r) => r.id)).toEqual(before);
  });

  it('catalogo vuoto → nessun gruppo', () => {
    expect(groupByMap([])).toEqual([]);
  });
});

describe('modeBlurb', () => {
  it('Standard: percorso più lungo', () => {
    expect(modeBlurb(usa)).toBe('33 biglietti in gioco · ricevi 4, tieni 2 · pesca 3, tieni 1 · Longest Path +10');
  });

  it('1910: Globetrotter', () => {
    expect(modeBlurb(usa1910)).toBe('33 biglietti in gioco · ricevi 4, tieni 2 · pesca 3, tieni 1 · Globetrotter +15');
  });

  it('senza bonus', () => {
    expect(modeBlurb(usaBig)).toBe('33 biglietti in gioco · ricevi 4, tieni 2 · pesca 4, tieni 1 · nessun bonus');
  });

  it('più bonus nell ordine del catalogo; i tipi sconosciuti sono ignorati', () => {
    expect(modeBlurb(usaMega)).toBe(
      '33 biglietti in gioco · ricevi 5, tieni 3 · pesca 4, tieni 1 · Longest Path +10 · Globetrotter +15',
    );
    expect(modeBlurb({ ...usa, bonuses: [{ type: 'futuro', label: 'Boh', points: 3 }] })).toBe(
      '33 biglietti in gioco · ricevi 4, tieni 2 · pesca 3, tieni 1 · nessun bonus',
    );
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
