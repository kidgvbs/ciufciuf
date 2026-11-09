import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createActionGate } from '../action-gate';

const MIN_MS = 350;
const MAX_MS = 3000;

describe('createActionGate', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function makeGate() {
    const onChange = vi.fn();
    return { gate: createActionGate({ minMs: MIN_MS, maxMs: MAX_MS, onChange }), onChange };
  }

  it('il primo tryAcquire blocca, il secondo è rifiutato', () => {
    const { gate } = makeGate();
    expect(gate.locked).toBe(false);
    expect(gate.tryAcquire()).toBe(true);
    expect(gate.locked).toBe(true);
    expect(gate.tryAcquire()).toBe(false);
  });

  it('rifiuta il secondo tryAcquire entro minMs anche se settle() è già arrivato', () => {
    const { gate } = makeGate();
    gate.tryAcquire();
    vi.advanceTimersByTime(20);
    gate.settle();
    vi.advanceTimersByTime(MIN_MS - 21);
    expect(gate.locked).toBe(true);
    expect(gate.tryAcquire()).toBe(false);
  });

  it('si sblocca allo scadere di minMs se la risposta è arrivata prima', () => {
    const { gate } = makeGate();
    gate.tryAcquire();
    gate.settle();
    vi.advanceTimersByTime(MIN_MS);
    expect(gate.locked).toBe(false);
    expect(vi.getTimerCount()).toBe(0);
    expect(gate.tryAcquire()).toBe(true);
  });

  it('si sblocca alla risposta se minMs è già trascorso', () => {
    const { gate } = makeGate();
    gate.tryAcquire();
    vi.advanceTimersByTime(MIN_MS + 100);
    expect(gate.locked).toBe(true);
    gate.settle();
    expect(gate.locked).toBe(false);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('non si sblocca prima della risposta se maxMs non è scaduto', () => {
    const { gate } = makeGate();
    gate.tryAcquire();
    vi.advanceTimersByTime(MAX_MS - 1);
    expect(gate.locked).toBe(true);
    expect(gate.tryAcquire()).toBe(false);
  });

  it('a maxMs si sblocca comunque, senza risposta', () => {
    const { gate } = makeGate();
    gate.tryAcquire();
    vi.advanceTimersByTime(MAX_MS);
    expect(gate.locked).toBe(false);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('un maxMs del blocco precedente non riapre il blocco successivo', () => {
    const { gate } = makeGate();
    gate.tryAcquire();
    gate.settle();
    vi.advanceTimersByTime(MAX_MS - 1);
    expect(gate.tryAcquire()).toBe(true);
    vi.advanceTimersByTime(1);
    expect(gate.locked).toBe(true);
  });

  it('una settle() del blocco precedente non vale per il successivo', () => {
    const { gate } = makeGate();
    gate.tryAcquire();
    gate.settle();
    vi.advanceTimersByTime(MIN_MS);
    gate.tryAcquire();
    vi.advanceTimersByTime(MIN_MS);
    expect(gate.locked).toBe(true);
  });

  it('chiama onChange su blocco e sblocco', () => {
    const { gate, onChange } = makeGate();
    gate.tryAcquire();
    expect(onChange.mock.calls).toEqual([[true]]);
    gate.tryAcquire();
    gate.settle();
    expect(onChange).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(MIN_MS);
    expect(onChange.mock.calls).toEqual([[true], [false]]);
  });

  it('settle() a blocco inattivo non fa nulla', () => {
    const { gate, onChange } = makeGate();
    gate.settle();
    expect(gate.locked).toBe(false);
    expect(onChange).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
    // Non deve valere come risposta anticipata del blocco successivo.
    gate.tryAcquire();
    vi.advanceTimersByTime(MIN_MS);
    expect(gate.locked).toBe(true);
  });

  it('dispose() cancella i timer e una settle() successiva non ha effetto', () => {
    const { gate, onChange } = makeGate();
    gate.tryAcquire();
    gate.dispose();
    expect(vi.getTimerCount()).toBe(0);
    expect(gate.locked).toBe(false);
    gate.settle();
    expect(vi.getTimerCount()).toBe(0);
    expect(gate.locked).toBe(false);
    expect(onChange.mock.calls).toEqual([[true]]);
  });
});
