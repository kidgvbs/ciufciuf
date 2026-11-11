// Caricamento di mappa e biglietti dal server (`GET /api/rulesets/{id}/map`).
//
// `loadRulesetMap` valida la risposta e memorizza le mappe già caricate per (host, ruleset);
// gli errori non vengono memorizzati. `createMapTracker` espone lo stato di caricamento per la
// partita corrente e ignora le risposte di richieste superate da una più recente.

import { isRecord } from './guards';
import { LobbyClient, MAP_FETCH_TIMEOUT_MS, MAP_TIMEOUT_MESSAGE } from './lobby';
import type { MapData } from './map-utils';

const mapCache = new Map<string, Promise<MapData>>();

function isArrayOfRecords(v: unknown): v is Record<string, unknown>[] {
  return Array.isArray(v) && v.every(isRecord);
}

/** `true` se `v` ha la forma minima di `MapData`: città con `name`, rotte con `len` 1–12, biglietti con `id` testuale. */
export function isMapData(v: unknown): v is MapData {
  if (!isRecord(v)) return false;
  const { cities, routes, tickets } = v;
  return (
    isArrayOfRecords(cities) &&
    isArrayOfRecords(routes) &&
    isArrayOfRecords(tickets) &&
    [...cities, ...routes, ...tickets].every((x) => typeof x.id === 'string') &&
    cities.every((c) => typeof c.name === 'string') &&
    routes.every(
      (r) =>
        typeof r.from === 'string' &&
        typeof r.to === 'string' &&
        typeof r.len === 'number' &&
        Number.isInteger(r.len) &&
        r.len >= 1 &&
        r.len <= 12 &&
        (r.double === undefined || typeof r.double === 'boolean') &&
        typeof r.color === 'string',
    ) &&
    tickets.every((t) => typeof t.from === 'string' && typeof t.to === 'string')
  );
}

/** Mappa del ruleset `rulesetId` servita da `host`; richieste concorrenti condividono la fetch. */
export function loadRulesetMap(host: string, rulesetId: string): Promise<MapData> {
  const key = `${host}\n${rulesetId}`;
  const cached = mapCache.get(key);
  if (cached) return cached;
  const pending = new LobbyClient(host).getRulesetMap(rulesetId).then((data) => {
    if (!isMapData(data)) throw new Error('Risposta mappa non valida');
    return data;
  });
  mapCache.set(key, pending);
  pending.catch(() => {
    if (mapCache.get(key) === pending) mapCache.delete(key);
  });
  return pending;
}

/** Svuota la cache delle mappe: lo fanno `track` e `reset` del tracker a ogni cambio di partita. */
export function clearMapCache(): void {
  mapCache.clear();
}

export type MapState =
  | { status: 'idle'; data: null; error: '' }
  | { status: 'loading'; data: null; error: '' }
  | { status: 'ready'; data: MapData; error: '' }
  | { status: 'error'; data: null; error: string };

export interface MapTracker {
  /** Svuota la cache e avvia il caricamento; una chiamata successiva rende obsoleto l'esito della precedente. */
  track(source: () => Promise<MapData>): void;
  /** Ripete l'ultimo `track` (dopo un errore). */
  retry(): void;
  /** Svuota la cache, torna a `idle` e ignora ogni caricamento in corso. */
  reset(): void;
}

export function createMapTracker(onChange: (state: MapState) => void): MapTracker {
  let generation = 0;
  let lastSource: (() => Promise<MapData>) | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;

  function track(source: () => Promise<MapData>): void {
    clearMapCache();
    clearTimeout(timer);
    lastSource = source;
    const mine = ++generation;
    onChange({ status: 'loading', data: null, error: '' });
    // L'intero caricamento (anche la risoluzione del ruleset) deve chiudersi entro il timeout.
    timer = setTimeout(() => {
      if (mine !== generation) return;
      generation++;
      onChange({ status: 'error', data: null, error: MAP_TIMEOUT_MESSAGE });
    }, MAP_FETCH_TIMEOUT_MS);
    source().then(
      (data) => {
        if (mine !== generation) return;
        clearTimeout(timer);
        onChange({ status: 'ready', data, error: '' });
      },
      (e: unknown) => {
        if (mine !== generation) return;
        clearTimeout(timer);
        const message = e instanceof Error ? e.message : String(e);
        onChange({ status: 'error', data: null, error: message });
      },
    );
  }

  return {
    track,
    retry() {
      if (lastSource) track(lastSource);
    },
    reset() {
      clearMapCache();
      clearTimeout(timer);
      generation++;
      lastSource = null;
      onChange({ status: 'idle', data: null, error: '' });
    },
  };
}
