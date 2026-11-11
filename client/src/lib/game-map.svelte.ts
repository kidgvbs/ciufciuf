// Mappa e biglietti della partita corrente, caricati dal server per `rulesetId`.

import { createMapTracker, loadRulesetMap, type MapState } from './map-loader';

class GameMapStore {
  state = $state.raw<MapState>({ status: 'idle', data: null, layout: null, error: '' });
  private tracker = createMapTracker((s) => {
    this.state = s;
  });

  /** Carica la mappa della partita; `resolveRulesetId` ottiene l'id del ruleset (es. da GET /api/games/{id}). */
  track(host: string, resolveRulesetId: () => Promise<string>): void {
    this.tracker.track(async () => loadRulesetMap(host, await resolveRulesetId()));
  }

  retry(): void {
    this.tracker.retry();
  }

  reset(): void {
    this.tracker.reset();
  }
}

/** Istanza singleton: mappa della partita a cui è agganciata la vista. */
export const gameMap = new GameMapStore();
