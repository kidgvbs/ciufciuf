import { describe, it, expect } from 'vitest';
import { isPrivateStateFresh, isSnapshotPrivateFresh } from '../dedup';
import type { PrivateState } from '../proto';

function makePrivateState(seq: bigint): PrivateState {
  return {
    seq,
    hand: [],
    tickets: [],
    offered_tickets: [],
    legal_moves: {
      routes: [],
      draw_face_up: [],
      can_draw_deck: false,
      can_draw_tickets: false,
    },
  };
}

describe('isPrivateStateFresh', () => {
  it('accetta seq maggiore', () => {
    expect(isPrivateStateFresh(5n, makePrivateState(6n))).toBe(true);
  });

  it('scarta seq uguale', () => {
    expect(isPrivateStateFresh(5n, makePrivateState(5n))).toBe(false);
  });

  it('scarta seq minore', () => {
    expect(isPrivateStateFresh(5n, makePrivateState(4n))).toBe(false);
  });

  it('accetta primo messaggio (lastSeq=0)', () => {
    expect(isPrivateStateFresh(0n, makePrivateState(1n))).toBe(true);
  });
});

describe('isSnapshotPrivateFresh', () => {
  it('accetta snapshot senza private (spettatore)', () => {
    expect(isSnapshotPrivateFresh(0n, null)).toBe(true);
  });

  it('accetta snapshot con seq maggiore', () => {
    expect(isSnapshotPrivateFresh(5n, makePrivateState(7n))).toBe(true);
  });

  it('scarta snapshot con seq uguale', () => {
    expect(isSnapshotPrivateFresh(7n, makePrivateState(7n))).toBe(false);
  });

  it('scarta snapshot con seq minore', () => {
    expect(isSnapshotPrivateFresh(7n, makePrivateState(5n))).toBe(false);
  });
});