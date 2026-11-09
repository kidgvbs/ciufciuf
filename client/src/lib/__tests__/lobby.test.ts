import { describe, it, expect, beforeEach } from 'vitest';
import {
  savePlayerName,
  loadPlayerName,
  saveLastGame,
  loadLastGame,
  clearLastGame,
} from '../lobby';

// Stub minimale di localStorage (environment node).
function makeStorage() {
  const map = new Map<string, string>();
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
  };
}

beforeEach(() => {
  (globalThis as Record<string, unknown>).localStorage = makeStorage();
});

describe('player name', () => {
  it('ritorna stringa vuota se mai salvato', () => {
    expect(loadPlayerName()).toBe('');
  });

  it('salva e ricarica', () => {
    savePlayerName('Gabriele');
    expect(loadPlayerName()).toBe('Gabriele');
  });
});

describe('last game', () => {
  it('ritorna null se mai salvata', () => {
    expect(loadLastGame()).toBeNull();
  });

  it('salva e ricarica gameId + hostToken', () => {
    saveLastGame({ gameId: 'ABC123', hostToken: 'tok' });
    expect(loadLastGame()).toEqual({ gameId: 'ABC123', hostToken: 'tok' });
  });

  it('ritorna null su JSON corrotto', () => {
    localStorage.setItem('ciufciuf-last-game', '{non-json');
    expect(loadLastGame()).toBeNull();
  });

  it('clearLastGame rimuove il record', () => {
    saveLastGame({ gameId: 'ABC123', hostToken: '' });
    clearLastGame();
    expect(loadLastGame()).toBeNull();
  });
});
