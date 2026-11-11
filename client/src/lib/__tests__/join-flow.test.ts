import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createJoinFlow,
  JOIN_TIMEOUT_MS,
  NAME_ALREADY_JOINED_MESSAGE,
  joinWithSavedIdentity,
} from '../join-flow';
import { LobbyError, type StoredIdentity } from '../lobby';

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(console, 'warn').mockImplementation(() => {});
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('createJoinFlow', () => {
  it('join_timeout_does_not_abort_request', async () => {
    const onSlow = vi.fn();
    const onError = vi.fn();
    const attempt = vi.fn(
      () => new Promise<void>((resolve) => setTimeout(resolve, JOIN_TIMEOUT_MS + 5_000)),
    );
    const flow = createJoinFlow({
      attempt,
      registeredNames: async () => [],
      name: 'Anna',
      onSlow,
      onError,
    });

    const result = flow.run();
    await vi.advanceTimersByTimeAsync(JOIN_TIMEOUT_MS);
    expect(onSlow).toHaveBeenCalledTimes(1);
    // La richiesta resta in corso e, se riesce, il join prosegue normalmente.
    await vi.advanceTimersByTimeAsync(5_000);
    expect(await result).toBe(true);
    expect(attempt).toHaveBeenCalledTimes(1);
    expect(onError).not.toHaveBeenCalled();
  });

  it('cancel_during_slow_join_ignores_late_result', async () => {
    const onSlow = vi.fn();
    const onError = vi.fn();
    let applied = 0;
    let fail = false;
    const flow = createJoinFlow({
      attempt: (isCancelled) =>
        new Promise<void>((resolve, reject) =>
          setTimeout(() => {
            if (fail) reject(new Error('Failed to fetch'));
            else {
              if (!isCancelled()) applied++;
              resolve();
            }
          }, JOIN_TIMEOUT_MS + 5_000),
        ),
      registeredNames: async () => ['Anna'],
      name: 'Anna',
      onSlow,
      onError,
    });

    const result = flow.run();
    await vi.advanceTimersByTimeAsync(JOIN_TIMEOUT_MS);
    expect(onSlow).toHaveBeenCalledTimes(1);
    flow.cancel();
    flow.cancel();
    // La fetch riesce dopo l'annullamento: esito ignorato e nessun errore mostrato.
    await vi.advanceTimersByTimeAsync(5_000);
    expect(await result).toBe(false);
    expect(applied).toBe(0);
    expect(onError).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);

    // Anche un fallimento tardivo resta silenzioso.
    fail = true;
    const second = createJoinFlow({
      attempt: () => Promise.reject(new Error('boom')),
      registeredNames: async () => [],
      name: 'Anna',
      onSlow: vi.fn(),
      onError,
    });
    const pending = second.run();
    second.cancel();
    expect(await pending).toBe(false);
    expect(onError).not.toHaveBeenCalled();
  });

  it('un join rapido non segnala lentezza e ferma il timer', async () => {
    const onSlow = vi.fn();
    const flow = createJoinFlow({
      attempt: async () => {},
      registeredNames: async () => [],
      name: 'Anna',
      onSlow,
      onError: vi.fn(),
    });
    expect(await flow.run()).toBe(true);
    await vi.advanceTimersByTimeAsync(JOIN_TIMEOUT_MS * 2);
    expect(onSlow).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('dopo il timeout un fallimento con nome già registrato chiede la riassegnazione', async () => {
    const onError = vi.fn();
    const flow = createJoinFlow({
      attempt: () =>
        new Promise<void>((_, reject) =>
          setTimeout(() => reject(new Error('Failed to fetch')), JOIN_TIMEOUT_MS + 1_000),
        ),
      registeredNames: async () => ['anna '],
      name: 'Anna',
      onSlow: vi.fn(),
      onError,
    });
    const result = flow.run();
    await vi.advanceTimersByTimeAsync(JOIN_TIMEOUT_MS + 1_000);
    expect(await result).toBe(false);
    expect(onError).toHaveBeenCalledExactlyOnceWith(NAME_ALREADY_JOINED_MESSAGE);
  });

  it('un fallimento rapido non interroga il server', async () => {
    const registeredNames = vi.fn(async () => ['Anna']);
    const onError = vi.fn();
    const flow = createJoinFlow({
      attempt: async () => {
        throw new Error('Nome già in uso');
      },
      registeredNames,
      name: 'Anna',
      onSlow: vi.fn(),
      onError,
    });
    expect(await flow.run()).toBe(false);
    expect(registeredNames).not.toHaveBeenCalled();
    expect(onError).toHaveBeenCalledWith('Nome già in uso');
  });
});

describe('joinWithSavedIdentity', () => {
  it('late_success_after_cancel_saves_identity', async () => {
    const store = new Map<string, StoredIdentity>();
    const remembered: boolean[] = [];
    const loadIdentity = (gameId: string) => store.get(gameId) ?? null;
    const saveIdentity = (identity: StoredIdentity, _now: number, remember: boolean) => {
      store.set(identity.gameId, identity);
      remembered.push(remember);
    };
    const tokens: (string | undefined)[] = [];
    let applied = 0;
    const onError = vi.fn();

    // Primo tentativo: la POST riesce solo dopo "Annulla".
    const flow = createJoinFlow({
      attempt: async (isCancelled) => {
        const result = await joinWithSavedIdentity({
          gameId: 'g1',
          host: 'http://h:1',
          name: 'Anna',
          isCancelled,
          post: (token) => {
            tokens.push(token);
            return new Promise((resolve) =>
              setTimeout(() => resolve({ playerId: 'p1', token: 'tok1' }), JOIN_TIMEOUT_MS + 5_000),
            );
          },
          loadIdentity,
          saveIdentity,
        });
        if (result) applied++;
      },
      registeredNames: async () => [],
      name: 'Anna',
      onSlow: vi.fn(),
      onError,
    });
    const first = flow.run();
    await vi.advanceTimersByTimeAsync(JOIN_TIMEOUT_MS);
    flow.cancel();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(await first).toBe(false);
    expect(applied).toBe(0);
    expect(onError).not.toHaveBeenCalled();
    expect(store.get('g1')).toMatchObject({
      playerId: 'p1',
      token: 'tok1',
      gameId: 'g1',
      host: 'http://h:1',
      name: 'Anna',
    });
    // Non diventa l'ultima partita: niente riconnessione automatica.
    expect(remembered).toEqual([false]);

    // Secondo tentativo con lo stesso nome (maiuscole/spazi diversi): il server risponde
    // NAME_TAKEN, si riusa il token salvato.
    const second = await joinWithSavedIdentity({
      gameId: 'g1',
      host: 'http://h:1',
      name: 'anna ',
      isCancelled: () => false,
      post: async (token) => {
        tokens.push(token);
        throw new LobbyError(409, 'Nome già in uso', 'NAME_TAKEN');
      },
      loadIdentity,
      saveIdentity,
    });
    expect(tokens).toEqual([undefined, 'tok1']);
    expect(second).toEqual({ playerId: 'p1', token: 'tok1' });

    // Un nome diverso non riusa l'identità salvata.
    const other = joinWithSavedIdentity({
      gameId: 'g1',
      host: 'http://h:1',
      name: 'Bruno',
      isCancelled: () => false,
      post: async () => {
        throw new LobbyError(409, 'Nome già in uso', 'NAME_TAKEN');
      },
      loadIdentity,
      saveIdentity,
    });
    await expect(other).rejects.toBeInstanceOf(LobbyError);
  });
});
