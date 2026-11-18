// Catalogo dei ruleset del server, condiviso dalle viste (selettore mappe, nome del bonus).

import {
  bigCities,
  globetrotterLabel,
  hasGlobetrotter,
  hasLongestPath,
  loadRulesets,
  longestPathLabel,
  rulesetTitleById,
  specialRules,
  type RulesetInfo,
} from './rulesets';

export type RulesetCatalogStatus = 'idle' | 'loading' | 'ready' | 'error';

class RulesetCatalogStore {
  status = $state<RulesetCatalogStatus>('idle');
  items = $state.raw<RulesetInfo[]>([]);
  error = $state<string>('');
  private seq = 0;

  /** Carica il catalogo; non fa nulla se è già pronto o in caricamento (dopo un errore ritenta). */
  load(host: string): void {
    if (this.status === 'loading' || this.status === 'ready') return;
    const mine = ++this.seq;
    this.status = 'loading';
    this.error = '';
    loadRulesets(host).then(
      (items) => {
        if (mine !== this.seq) return;
        this.items = items;
        this.status = 'ready';
      },
      (e: unknown) => {
        if (mine !== this.seq) return;
        console.warn('[rulesets] catalogo non disponibile:', e);
        this.error = e instanceof Error ? e.message : String(e);
        this.status = 'error';
      },
    );
  }

  /** Nome del bonus percorso più lungo per `rulesetId` ('Longest Path' se il catalogo non lo conosce). */
  longestPathLabel(rulesetId: string | null | undefined): string {
    return longestPathLabel(this.items, rulesetId);
  }

  /** Nome del bonus Globetrotter per `rulesetId` ('Globetrotter' se il catalogo non lo conosce). */
  globetrotterLabel(rulesetId: string | null | undefined): string {
    return globetrotterLabel(this.items, rulesetId);
  }

  /** Il ruleset prevede il bonus percorso più lungo (catalogo non pronto o ruleset ignoto → sì). */
  hasLongestPath(rulesetId: string | null | undefined): boolean {
    return hasLongestPath(this.items, rulesetId);
  }

  /** Il ruleset prevede il bonus Globetrotter. */
  hasGlobetrotter(rulesetId: string | null | undefined): boolean {
    return hasGlobetrotter(this.items, rulesetId);
  }

  /** Ids delle grandi città del ruleset (vuoto se nessuna o ruleset ignoto). */
  bigCities(rulesetId: string | null | undefined): string[] {
    return bigCities(this.items, rulesetId);
  }

  /** Regole speciali del ruleset (vuoto se nessuna, ruleset ignoto o catalogo non pronto). */
  specialRules(rulesetId: string | null | undefined): string[] {
    return specialRules(this.items, rulesetId);
  }

  /** Titolo del ruleset («USA · 1910») o `null` se il catalogo non lo conosce. */
  title(rulesetId: string | null | undefined): string | null {
    return rulesetTitleById(this.items, rulesetId);
  }
}

/** Istanza singleton del catalogo. */
export const rulesetCatalog = new RulesetCatalogStore();
