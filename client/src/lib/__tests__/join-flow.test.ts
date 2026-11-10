import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createJoinFlow,
  JOIN_TIMEOUT_MS,
  NAME_ALREADY_JOINED_MESSAGE,
} from '../join-flow';

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
