// Catalogo mappe/ruleset disponibili lato client (selettore "Nuova partita").
//
// Lista statica per ora (LAN personale): il POST /api/games usa il rulesetId
// selezionato. Le card "locked" sono annunci di mappe future, non creabili.
// Vincolo regola d'oro 2 (zero IP del titolare del gioco originale):
// nessun asset o nome ufficiale — le thumbnail sono mini-render SVG
// originali dalle nostre coordinate (MapCard).

export interface RulesetInfo {
  /** Id passato a POST /api/games (deve esistere in data/rulesets/). */
  id: string;
  /** Nome mostrato sulla card. */
  name: string;
  /** Sottotitolo descrittivo. */
  blurb: string;
  /** true = selezionabile e creabile; false = card bloccata "In arrivo". */
  available: boolean;
  /** Nome del bonus per il percorso più lungo (varia per mappa). */
  bonusLabel: string;
}

export const RULESETS: RulesetInfo[] = [
  {
    id: 'usa_v2',
    name: 'USA',
    blurb: 'La mappa classica transcontinentale — 36 città, 100 rotte',
    available: true,
    bonusLabel: 'Longest Path',
  },
  {
    id: 'europe',
    name: 'Europe',
    blurb: 'Il vecchio continente — 47 città, 101 rotte, con tunnel, traghetti e stazioni',
    available: true,
    bonusLabel: 'European Express',
  },
];

/** Ruleset selezionato di default nel form di creazione. */
export const DEFAULT_RULESET_ID = 'usa_v2';

const DEFAULT_BONUS_LABEL = 'Longest Path';

/** Nome del bonus percorso più lungo per `rulesetId`; 'Longest Path' se il ruleset è sconosciuto o assente. */
export function bonusLabel(rulesetId: string | null | undefined): string {
  return RULESETS.find((r) => r.id === rulesetId)?.bonusLabel ?? DEFAULT_BONUS_LABEL;
}
