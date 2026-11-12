import { describe, it, expect } from 'vitest';
import {
  DRAW_SOURCE_TTL_MS,
  addedColors,
  countByColor,
  isFreshDrawSource,
} from '../draw-animation';

describe('countByColor', () => {
  it('conta le carte per colore', () => {
    const counts = countByColor(['RED', 'BLUE', 'RED', 'LOCOMOTIVE']);
    expect(counts.get('RED')).toBe(2);
    expect(counts.get('BLUE')).toBe(1);
    expect(counts.get('LOCOMOTIVE')).toBe(1);
    expect(counts.get('GREEN')).toBeUndefined();
  });

  it('mano vuota → nessun colore', () => {
    expect(countByColor([]).size).toBe(0);
  });
});

describe('addedColors', () => {
  it('trova il colore pescato (nuovo o già in mano)', () => {
    expect(addedColors(countByColor(['RED']), countByColor(['RED', 'BLUE']))).toEqual(['BLUE']);
    expect(addedColors(countByColor(['RED']), countByColor(['RED', 'RED']))).toEqual(['RED']);
  });

  it('nessuna crescita (stessa mano, carte spese) → niente', () => {
    expect(addedColors(countByColor(['RED', 'BLUE']), countByColor(['RED', 'BLUE']))).toEqual([]);
    expect(addedColors(countByColor(['RED', 'RED', 'BLUE']), countByColor(['BLUE']))).toEqual([]);
  });

  it('più colori aumentati, nell ordine della nuova mano', () => {
    expect(
      addedColors(countByColor([]), countByColor(['GREEN', 'RED', 'GREEN'])),
    ).toEqual(['GREEN', 'RED']);
  });
});

describe('isFreshDrawSource', () => {
  const rect = { left: 0, top: 0, width: 10, height: 10 };

  it('è valida entro il TTL', () => {
    expect(isFreshDrawSource({ rect, at: 1000 }, 1000)).toBe(true);
    expect(isFreshDrawSource({ rect, at: 1000 }, 1000 + DRAW_SOURCE_TTL_MS)).toBe(true);
  });

  it('è stantia oltre il TTL o se il clock è tornato indietro', () => {
    expect(isFreshDrawSource({ rect, at: 1000 }, 1001 + DRAW_SOURCE_TTL_MS)).toBe(false);
    expect(isFreshDrawSource({ rect, at: 1000 }, 999)).toBe(false);
  });
});
