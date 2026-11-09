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
}

export const RULESETS: RulesetInfo[] = [
  {
    id: 'usa_v2',
    name: 'USA',
    blurb: 'La mappa classica transcontinentale — 36 città, 100 rotte',
    available: true,
  },
  {
    id: 'europe',
    name: 'Europe',
    blurb: 'Tunnel, traghetti e stazioni',
    available: false,
  },
];

/** Ruleset selezionato di default nel form di creazione. */
export const DEFAULT_RULESET_ID = 'usa_v2';
