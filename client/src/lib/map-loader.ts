// Caricamento di mappa e biglietti dal server (`GET /api/rulesets/{id}/map`).
//
// `loadRulesetMap` valida la risposta e memorizza le mappe già caricate per (host, ruleset);
// gli errori non vengono memorizzati. `createMapTracker` espone lo stato di caricamento per la
// partita corrente e ignora le risposte di richieste superate da una più recente.

import { isRecord } from './guards';
import { LobbyClient } from './lobby';
import type { MapData } from './map-utils';

const mapCache = new Map<string, Promise<MapData>>();

function isArrayOfRecords(v: unknown): v is Record<string, unknown>[] {
  return Array.isArray(v) && v.every(isRecord);
}

/** `true` se `v` ha la forma minima di `MapData`: città, rotte e biglietti con `id` testuale. */
export function isMapData(v: unknown): v is MapData {
  if (!isRecord(v)) return false;
  const { cities, routes, tickets } = v;
  return (
    isArrayOfRecords(cities) &&
    isArrayOfRecords(routes) &&
    isArrayOfRecords(tickets) &&
    [...cities, ...routes, ...tickets].every((x) => typeof x.id === 'string') &&
    routes.every((r) => typeof r.from === 'string' && typeof r.to === 'string') &&
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

/** Svuota la cache delle mappe (test). */
export function clearMapCache(): void {
  mapCache.clear();
}

export type MapState =
  | { status: 'idle'; data: null; error: '' }
  | { status: 'loading'; data: null; error: '' }
  | { status: 'ready'; data: MapData; error: '' }
  | { status: 'error'; data: null; error: string };

export interface MapTracker {
  /** Avvia il caricamento; una chiamata successiva rende obsoleto l'esito della precedente. */
  track(source: () => Promise<MapData>): void;
  /** Ripete l'ultimo `track` (dopo un errore). */
  retry(): void;
  /** Torna a `idle` e ignora ogni caricamento in corso. */
  reset(): void;
}

export function createMapTracker(onChange: (state: MapState) => void): MapTracker {
  let generation = 0;
  let lastSource: (() => Promise<MapData>) | null = null;

  function track(source: () => Promise<MapData>): void {
    lastSource = source;
    const mine = ++generation;
    onChange({ status: 'loading', data: null, error: '' });
    source().then(
      (data) => {
        if (mine === generation) onChange({ status: 'ready', data, error: '' });
      },
      (e: unknown) => {
        if (mine !== generation) return;
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
      generation++;
      lastSource = null;
      onChange({ status: 'idle', data: null, error: '' });
    },
  };
}
