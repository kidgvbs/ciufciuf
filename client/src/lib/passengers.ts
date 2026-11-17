// Passeggeri (Germany 2017): quali estremi di una rotta chiedono la scelta del colore e come
// mostrarli. Il server decide tutto il resto (colori presenti, punti di maggioranza).

import type { CardColor, CityPassengers, PassengerCount, PassengerPick } from './proto';
import { CARD_COLOR_CSS } from './colors';

const PASSENGER_COLOR_NAME: Record<string, string> = {
  Red: 'rosso',
  Black: 'nero',
  Green: 'verde',
  Yellow: 'giallo',
  Blue: 'blu',
  White: 'bianco',
};

/** Nome italiano del colore di un passeggero (il ruleset usa nomi inglesi). */
export function passengerColorName(color: string): string {
  return PASSENGER_COLOR_NAME[color] ?? color;
}

/** Colore CSS di un passeggero: stessa palette delle carte quando il nome coincide. */
export function passengerColorCss(color: string): string {
  return CARD_COLOR_CSS[color.toUpperCase() as CardColor] ?? '#8a7a68';
}

/** Estremi della rotta con passeggeri di più colori: lì il giocatore sceglie quale prendere. */
export function passengerChoices(
  onCities: ReadonlyArray<CityPassengers>,
  route: { from: string; to: string },
): Array<{ cityId: string; passengers: PassengerCount[] }> {
  return [route.from, route.to].flatMap((cityId) => {
    const city = onCities.find((c) => c.city_id === cityId);
    return city && city.passengers.length > 1
      ? [{ cityId, passengers: city.passengers }]
      : [];
  });
}

/**
 * Scelte da inviare con `CLAIM_ROUTE`: il colore scelto per ogni estremo che lo richiede
 * (il primo se il giocatore non ha toccato nulla); `undefined` se non serve scegliere.
 */
export function passengerPicks(
  choices: ReadonlyArray<{ cityId: string; passengers: PassengerCount[] }>,
  picked: Readonly<Record<string, string>>,
): PassengerPick[] | undefined {
  if (choices.length === 0) return undefined;
  return choices.flatMap((c) => {
    const chosen = picked[c.cityId];
    const color = c.passengers.some((p) => p.color === chosen) ? chosen : c.passengers[0]?.color;
    return color === undefined ? [] : [{ city_id: c.cityId, color }];
  });
}
