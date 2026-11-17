// Caricamento di mappa, biglietti e layout schematico dal server (`GET /api/rulesets/{id}/map` e
// `/layout`).
//
// `loadRulesetMap` valida le risposte e memorizza le mappe già caricate per (host, ruleset);
// gli errori non vengono memorizzati. `createMapTracker` espone lo stato di caricamento per la
// partita corrente e ignora le risposte di richieste superate da una più recente.

import { isRecord } from './guards';
import { LobbyClient, LobbyError, MAP_FETCH_TIMEOUT_MS, MAP_TIMEOUT_MESSAGE } from './lobby';
import type { MapData, MapLayout } from './map-utils';

const mapCache = new Map<string, Promise<RulesetMap>>();

function isArrayOfRecords(v: unknown): v is Record<string, unknown>[] {
  return Array.isArray(v) && v.every(isRecord);
}

/** `true` se `v` ha la forma minima di `MapData`: città con `name`, rotte con `len` 1–12 (e `tunnel`/`ferries`/`alwaysOpen` opzionali coerenti, `country` booleano sulle città), biglietti con `id` testuale. */
export function isMapData(v: unknown): v is MapData {
  if (!isRecord(v)) return false;
  const { cities, routes, tickets } = v;
  return (
    isArrayOfRecords(cities) &&
    isArrayOfRecords(routes) &&
    isArrayOfRecords(tickets) &&
    [...cities, ...routes, ...tickets].every((x) => typeof x.id === 'string') &&
    cities.every(
      (c) => typeof c.name === 'string' && (c.country === undefined || typeof c.country === 'boolean'),
    ) &&
    routes.every(
      (r) =>
        typeof r.from === 'string' &&
        typeof r.to === 'string' &&
        typeof r.len === 'number' &&
        Number.isInteger(r.len) &&
        r.len >= 1 &&
        r.len <= 12 &&
        (r.double === undefined || typeof r.double === 'boolean') &&
        (r.alwaysOpen === undefined || typeof r.alwaysOpen === 'boolean') &&
        (r.tunnel === undefined || typeof r.tunnel === 'boolean') &&
        (r.ferries === undefined ||
          (typeof r.ferries === 'number' &&
            Number.isInteger(r.ferries) &&
            r.ferries >= 0 &&
            r.ferries <= r.len)) &&
        typeof r.color === 'string',
    ) &&
    tickets.every((t) => typeof t.from === 'string' && typeof t.to === 'string')
  );
}

function isNumber(v: unknown): v is number {
  return typeof v === 'number' && Number.isFinite(v);
}

/** `true` se `v` è un oggetto semplice i cui valori soddisfano `check`. */
function isRecordOf(v: unknown, check: (x: unknown) => boolean): v is Record<string, unknown> {
  return isRecord(v) && !Array.isArray(v) && Object.values(v).every(check);
}

const LABEL_ANCHORS: readonly unknown[] = ['start', 'middle', 'end'];

// Limiti del layout, gli stessi che applica il server.
const LAYOUT_MAX_SIZE = 100_000;
const LAYOUT_MAX_COORD = 100_000;
const LAYOUT_MAX_SCALE = 1000;

function isCoord(v: unknown): v is number {
  return isNumber(v) && Math.abs(v) <= LAYOUT_MAX_COORD;
}

function isSize(v: unknown): v is number {
  return isNumber(v) && v > 0 && v <= LAYOUT_MAX_SIZE;
}

function isScale(v: unknown): v is number {
  return isNumber(v) && v >= 0 && v <= LAYOUT_MAX_SCALE;
}

/**
 * `true` se `v` ha la forma di `MapLayout` entro i limiti del server: `width`/`height` in (0, 100000],
 * città `{x, y}`, etichette `{dx, dy, anchor}` e rotte con 2 o 3 punti `[x, y]` con coordinate finite
 * e |v| ≤ 100000; `font`, `doubleGap` e `unit` opzionali ma tipizzati (`doubleGap` e `unit` in [0, 1000]).
 */
export function isMapLayout(v: unknown): v is MapLayout {
  if (!isRecord(v)) return false;
  const { width, height, cities, labels, routes, font, doubleGap, unit } = v;
  return (
    isSize(width) &&
    isSize(height) &&
    isRecordOf(cities, (p) => isRecord(p) && isCoord(p.x) && isCoord(p.y)) &&
    isRecordOf(
      labels,
      (l) => isRecord(l) && isCoord(l.dx) && isCoord(l.dy) && LABEL_ANCHORS.includes(l.anchor),
    ) &&
    isRecordOf(
      routes,
      (pts) =>
        Array.isArray(pts) &&
        (pts.length === 2 || pts.length === 3) &&
        pts.every((p) => Array.isArray(p) && p.length === 2 && p.every(isCoord)),
    ) &&
    (font === undefined ||
      (isRecord(font) &&
        typeof font.family === 'string' &&
        isNumber(font.weight) &&
        isNumber(font.size))) &&
    (doubleGap === undefined || isScale(doubleGap)) &&
    (unit === undefined || isScale(unit))
  );
}

/** `true` se `layout` ha posizione ed etichetta per ogni città e una polyline per ogni rotta di `map`. */
export function layoutCoversMap(map: MapData, layout: MapLayout): boolean {
  const has = (record: object, id: string) => Object.hasOwn(record, id);
  return (
    map.cities.every((c) => has(layout.cities, c.id) && has(layout.labels, c.id)) &&
    map.routes.every((r) => has(layout.routes, r.id))
  );
}

/**
 * Mappa e layout del ruleset `rulesetId` serviti da `host`; richieste concorrenti condividono la fetch.
 * Un layout assente sul server (`LAYOUT_NOT_FOUND`) dà `layout: null` con la mappa utilizzabile;
 * ogni altro errore sul layout (non valido, incompleto, timeout, 5xx) fa fallire il caricamento.
 */
export function loadRulesetMap(host: string, rulesetId: string): Promise<RulesetMap> {
  const key = `${host}\n${rulesetId}`;
  const cached = mapCache.get(key);
  if (cached) return cached;
  const client = new LobbyClient(host);
  const layoutOrAbsent = client.getRulesetLayout(rulesetId).catch((e: unknown) => {
    if (e instanceof LobbyError && e.code === 'LAYOUT_NOT_FOUND') return null;
    throw e;
  });
  const pending = Promise.all([client.getRulesetMap(rulesetId), layoutOrAbsent]).then(
    ([map, layout]): RulesetMap => {
      if (!isMapData(map)) throw new Error('Risposta mappa non valida');
      if (layout === null) return { map, layout: null };
      if (!isMapLayout(layout)) throw new Error('Layout della mappa non valido');
      if (!layoutCoversMap(map, layout)) throw new Error('Layout della mappa incompleto');
      return { map, layout };
    },
  );
  mapCache.set(key, pending);
  pending.catch(() => {
    if (mapCache.get(key) === pending) mapCache.delete(key);
  });
  return pending;
}

/** Svuota la cache delle mappe: la svuota `track` del tracker a ogni nuova partita; `reset` la conserva. */
export function clearMapCache(): void {
  mapCache.clear();
}

/** Mappa di un ruleset con il suo layout schematico; `layout` è `null` se il server non ne ha uno per quella mappa. */
export interface RulesetMap {
  map: MapData;
  layout: MapLayout | null;
}

export type MapState =
  | { status: 'idle'; data: null; layout: null; error: '' }
  | { status: 'loading'; data: null; layout: null; error: '' }
  | { status: 'ready'; data: MapData; layout: MapLayout | null; error: '' }
  | { status: 'error'; data: null; layout: null; error: string };

export interface MapTracker {
  /** Svuota la cache e avvia il caricamento; una chiamata successiva rende obsoleto l'esito della precedente. */
  track(source: () => Promise<RulesetMap>): void;
  /** Ripete l'ultimo `track` (dopo un errore). */
  retry(): void;
  /** Torna a `idle` e ignora ogni caricamento in corso; la cache resta, così un rimontaggio non rifà la fetch. */
  reset(): void;
}

export function createMapTracker(onChange: (state: MapState) => void): MapTracker {
  let generation = 0;
  let lastSource: (() => Promise<RulesetMap>) | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;

  function track(source: () => Promise<RulesetMap>): void {
    clearMapCache();
    clearTimeout(timer);
    lastSource = source;
    const mine = ++generation;
    onChange({ status: 'loading', data: null, layout: null, error: '' });
    // L'intero caricamento (anche la risoluzione del ruleset) deve chiudersi entro il timeout.
    timer = setTimeout(() => {
      if (mine !== generation) return;
      generation++;
      onChange({ status: 'error', data: null, layout: null, error: MAP_TIMEOUT_MESSAGE });
    }, MAP_FETCH_TIMEOUT_MS);
    source().then(
      ({ map, layout }) => {
        if (mine !== generation) return;
        clearTimeout(timer);
        onChange({ status: 'ready', data: map, layout, error: '' });
      },
      (e: unknown) => {
        if (mine !== generation) return;
        clearTimeout(timer);
        const message = e instanceof Error ? e.message : String(e);
        onChange({ status: 'error', data: null, layout: null, error: message });
      },
    );
  }

  return {
    track,
    retry() {
      if (lastSource) track(lastSource);
    },
    reset() {
      clearTimeout(timer);
      generation++;
      lastSource = null;
      onChange({ status: 'idle', data: null, layout: null, error: '' });
    },
  };
}
