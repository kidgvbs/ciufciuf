import { describe, it, expect } from 'vitest';
import { parseWithBigInt, stringifyWithBigInt } from '../json';

describe('parseWithBigInt', () => {
  it('converte seq e client_seq in bigint', () => {
    const input = '{"seq": 42, "client_seq": 7, "name": "test"}';
    const result = parseWithBigInt(input) as Record<string, unknown>;
    expect(typeof result.seq).toBe('bigint');
    expect(result.seq).toBe(42n);
    expect(typeof result.client_seq).toBe('bigint');
    expect(result.client_seq).toBe(7n);
    expect(result.name).toBe('test');
  });

  it('lascia stare i numeri in altre chiavi', () => {
    const input = '{"score": 100, "trains_remaining": 30}';
    const result = parseWithBigInt(input) as Record<string, unknown>;
    expect(typeof result.score).toBe('number');
    expect(typeof result.trains_remaining).toBe('number');
  });

  it('gestisce campi annidati (server non annida, ma per robustezza)', () => {
    // Il server non annida seq in oggetti interni, ma verifichiamo il
    // comportamento di JSON.parse con reviver.
    const input = '{"type":"PRIVATE_STATE","payload":{"seq":128,"hand":["RED"]}}';
    const result = parseWithBigInt(input) as Record<string, unknown>;
    const payload = result.payload as Record<string, unknown>;
    expect(typeof payload.seq).toBe('bigint');
    expect(payload.seq).toBe(128n);
  });
});

describe('stringifyWithBigInt', () => {
  it('converte bigint in number per JSON', () => {
    const obj = { seq: 99n, name: 'test' };
    const json = stringifyWithBigInt(obj);
    const parsed = JSON.parse(json);
    expect(parsed.seq).toBe(99);
    expect(parsed.name).toBe('test');
  });

  it('lascia stare numeri normali', () => {
    const obj = { score: 42, name: 'test' };
    const json = stringifyWithBigInt(obj);
    const parsed = JSON.parse(json);
    expect(parsed.score).toBe(42);
  });

  it('arrotonda valori null', () => {
    const obj = { seq: null, name: 'test' };
    const json = stringifyWithBigInt(obj);
    const parsed = JSON.parse(json);
    expect(parsed.seq).toBeNull();
  });
});

describe('round-trip', () => {
  it('ServerMessage SNAPSHOT con private', () => {
    const input = JSON.stringify({
      type: 'SNAPSHOT',
      payload: {
        protocol_version: 1,
        public: {
          game_id: 'g1',
          protocol_version: 1,
          state: 'PLAYER_TURN',
          current_turn_player_id: 'p1',
          last_round: false,
          last_round_trigger_player_id: null,
          visible_deck: ['RED', null, 'BLUE'],
          claimed_routes: [],
          players_summary: [],
          tunnel_reveal: null,
          stations: [],
          final_scores: null,
          longest_path_player_ids: null,
        },
        private: {
          seq: 7,
          hand: ['RED'],
          tickets: [],
          offered_tickets: [],
          legal_moves: {
            routes: [],
            draw_face_up: [0],
            can_draw_deck: true,
            can_draw_tickets: false,
          },
        },
      },
    });
    const parsed = parseWithBigInt(input) as { type: string; payload: { private: { seq: bigint } } };
    expect(typeof parsed.payload.private.seq).toBe('bigint');
    expect(parsed.payload.private.seq).toBe(7n);

    // Re-serializza
    const out = stringifyWithBigInt(parsed);
    const reParsed = JSON.parse(out);
    expect(reParsed.payload.private.seq).toBe(7);
  });
});