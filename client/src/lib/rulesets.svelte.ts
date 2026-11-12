// Catalogo dei ruleset del server, condiviso dalle viste (selettore mappe, nome del bonus).

import { bonusLabel, loadRulesets, type RulesetInfo } from './rulesets';

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
  bonusLabel(rulesetId: string | null | undefined): string {
    return bonusLabel(this.items, rulesetId);
  }
}

/** Istanza singleton del catalogo. */
export const rulesetCatalog = new RulesetCatalogStore();
