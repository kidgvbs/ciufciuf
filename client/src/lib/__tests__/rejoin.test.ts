import { describe, it, expect } from 'vitest';
import { buildRejoinUrl, parseRejoinHash } from '../rejoin';

describe('rejoin', () => {
  const identity = { gameId: 'g 1', playerId: 'p/1', token: 'a+b&c' };

  it('l URL porta l identità nel fragment e si legge di nuovo', () => {
    const url = buildRejoinUrl('http://192.168.1.5:8080/', identity);
    expect(url.startsWith('http://192.168.1.5:8080/controller#')).toBe(true);
    const hash = url.slice(url.indexOf('#'));
    expect(parseRejoinHash(hash)).toEqual(identity);
    expect(url).not.toContain('?');
  });

  it('un fragment incompleto non è un identità', () => {
    expect(parseRejoinHash('')).toBeNull();
    expect(parseRejoinHash('#gameId=g&playerId=p')).toBeNull();
  });
});
