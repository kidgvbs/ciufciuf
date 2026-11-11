import { describe, it, expect } from 'vitest';
import { formatSavedAt } from '../saves';

describe('formatSavedAt', () => {
  it('formatta una data RFC 3339 in it-IT, senza il testo grezzo', () => {
    const text = formatSavedAt('2026-11-10T12:00:00Z');
    expect(text).toMatch(/^\d{2}\/\d{2}\/2026/);
    expect(text).not.toContain('T');
  });

  it('una data non valida resta com’è invece di mostrare "Invalid Date"', () => {
    expect(formatSavedAt('ieri sera')).toBe('ieri sera');
  });
});
