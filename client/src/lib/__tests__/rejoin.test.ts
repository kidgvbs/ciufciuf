import { describe, it, expect } from 'vitest';
import { buildRejoinUrl, parseRejoinHash, pruneRejoinLinks } from '../rejoin';

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

describe('pruneRejoinLinks', () => {
  const links = { Anna: 'http://x/a', Bruno: 'http://x/b' };

  it('non cambia nulla (stesso oggetto) se tutti aspettano ancora di rientrare', () => {
    const players = [
      { name: 'Anna', connected: false },
      { name: 'Bruno', connected: false },
    ];
    expect(pruneRejoinLinks(links, players, [])).toBe(links);
  });

  it('toglie la voce di chi è tornato connesso', () => {
    const players = [
      { name: 'Anna', connected: true },
      { name: 'Bruno', connected: false },
    ];
    expect(pruneRejoinLinks(links, players, [])).toEqual({ Bruno: 'http://x/b' });
  });

  it('toglie la voce di chi è uscito dal roster', () => {
    expect(pruneRejoinLinks(links, [{ name: 'Anna', connected: false }], [])).toEqual({
      Anna: 'http://x/a',
    });
  });

  it('in lobby di resume tiene i nomi del salvataggio non ancora rientrati', () => {
    expect(pruneRejoinLinks(links, [], ['Anna', 'Bruno'])).toBe(links);
    expect(
      pruneRejoinLinks(links, [{ name: 'Bruno', connected: true }], ['Anna']),
    ).toEqual({ Anna: 'http://x/a' });
  });

  it('senza link restituisce lo stesso oggetto', () => {
    const empty = {};
    expect(pruneRejoinLinks(empty, [], [])).toBe(empty);
  });
});
