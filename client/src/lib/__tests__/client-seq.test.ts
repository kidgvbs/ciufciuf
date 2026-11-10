import { describe, it, expect } from 'vitest';
import { createClientSeqSource, type SeqStorage } from '../client-seq';

function makeStorage(): SeqStorage {
  const map = new Map<string, string>();
  return {
    getItem: (k) => map.get(k) ?? null,
    setItem: (k, v) => void map.set(k, v),
  };
}

describe('client seq', () => {
  it('parte da now * 1000', () => {
    const source = createClientSeqSource(() => makeStorage(), () => 1_700_000_000_000);
    expect(source.next('p1')).toBe(1_700_000_000_000_000n);
  });

  it('è strettamente crescente anche nello stesso millisecondo', () => {
    const source = createClientSeqSource(() => makeStorage(), () => 1000);
    const values = [source.next('p1'), source.next('p1'), source.next('p1')];
    expect(values).toEqual([1_000_000n, 1_000_001n, 1_000_002n]);
  });

  it('client_seq_strictly_increasing_across_reload', () => {
    const storage = makeStorage();
    const before = createClientSeqSource(() => storage, () => 5000);
    const last = [before.next('p1'), before.next('p1'), before.next('p1')].at(-1)!;

    // Reload: nuova sorgente (memoria vuota) con l'orologio tornato indietro.
    const after = createClientSeqSource(() => storage, () => 4000);
    const first = after.next('p1');
    expect(first).toBeGreaterThan(last);
    expect(after.next('p1')).toBeGreaterThan(first);

    // Reload con l'orologio avanti: ripartire dall'orologio resta valido.
    const later = createClientSeqSource(() => storage, () => 9000);
    expect(later.next('p1')).toBe(9_000_000n);
  });

  it('i giocatori hanno contatori indipendenti', () => {
    const storage = makeStorage();
    const source = createClientSeqSource(() => storage, () => 1000);
    source.next('p1');
    source.next('p1');
    expect(source.next('p2')).toBe(1_000_000n);
  });

  it('senza storage resta crescente in memoria', () => {
    const source = createClientSeqSource(() => null, () => 1000);
    expect(source.next('p1')).toBeLessThan(source.next('p1'));
  });

  it('storage che lancia non interrompe la numerazione', () => {
    const broken: SeqStorage = {
      getItem: () => { throw new Error('SecurityError'); },
      setItem: () => { throw new Error('QuotaExceededError'); },
    };
    const source = createClientSeqSource(() => broken, () => 1000);
    const a = source.next('p1');
    expect(source.next('p1')).toBeGreaterThan(a);
  });

  it('ignora un valore salvato non numerico', () => {
    const storage = makeStorage();
    storage.setItem('ciufciuf-client-seq:p1', 'abc');
    const source = createClientSeqSource(() => storage, () => 1000);
    expect(source.next('p1')).toBe(1_000_000n);
  });
});
