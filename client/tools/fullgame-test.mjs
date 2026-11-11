#!/usr/bin/env bun
// fullgame-test.mjs — Partita completa E2E via REST+WS con due bot.
//
// Gioca una partita intera fino a GAME_OVER: draft iniziale e mid-game,
// turni alternati, claim rotte, pesca mazzo/face-up, ultimo giro, punteggi.
//
// Uso: bun client/tools/fullgame-test.mjs [base=http://localhost:18956]
//   --spawn-server  avvia automaticamente ttr-server su :18956
//   --ruleset <id>  ruleset da giocare (default usa_v2; `europe` aggiunge stazioni e tunnel)

import { readFileSync, mkdtempSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { spawn } from 'node:child_process';
import { tmpdir } from 'node:os';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, '..', '..');

// ── Config ──────────────────────────────────────────────────────────────
const SPAWN_SERVER = process.argv.includes('--spawn-server');
const PORT = 18956;
let BASE = `http://localhost:${PORT}`;
for (const a of process.argv) {
  if (a.startsWith('http://') || a.startsWith('https://')) BASE = a.replace(/\/$/, '');
}
const WS_BASE = BASE.replace(/^http/, 'ws');
const SEED = 42;
const MAX_TURNS = 600;
const RULESET = process.argv.includes('--ruleset') ? process.argv[process.argv.indexOf('--ruleset') + 1] : 'usa_v2';
const EUROPE = RULESET === 'europe';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const fail = (msg) => {
  console.error(`❌ FAIL: ${msg}`);
  cleanup();
  process.exit(1);
};
const assert = (cond, msg) => { if (!cond) fail(msg); };

// ── Server lifecycle ────────────────────────────────────────────────────
let serverProc = null;
let tmpDir = null;

async function startServer() {
  if (!SPAWN_SERVER) {
    // Quick health-check on the user-supplied BASE
    for (let i = 0; i < 20; i++) {
      try { await fetch(`${BASE}/api/games`); return; }
      catch { await sleep(500); }
    }
    fail(`Server non raggiungibile su ${BASE} dopo 10s`);
    return;
  }

  tmpDir = mkdtempSync(join(tmpdir(), 'ciufciuf-fullgame-'));
  const bin = join(ROOT, 'target', 'release', 'ttr-server');
  const args = ['--port', String(PORT), '--client-dist', join(ROOT, 'client', 'dist'), '--saves-dir', tmpDir, '--allow-client-seed'];
  console.log(`   Avvio server: ${bin} ${args.join(' ')}`);

  let stderr = '';
  serverProc = spawn(bin, args, { stdio: ['ignore', 'ignore', 'pipe'], cwd: ROOT });
  serverProc.stderr?.on('data', (d) => { stderr += d.toString(); });

  // Wait for server to become ready or die
  for (let i = 0; i < 60; i++) {
    if (serverProc.exitCode !== null) {
      fail(`Server morto (exit=${serverProc.exitCode}) all'avvio:\n${stderr.slice(-2000)}`);
    }
    try { await fetch(`${BASE}/api/games`); break; }
    catch { await sleep(500); }
  }
  if (serverProc.exitCode !== null) {
    fail(`Server morto durante wait-ready:\n${stderr.slice(-2000)}`);
  }
  console.log('   Server pronto');
}

function cleanup() {
  if (serverProc) {
    serverProc.kill('SIGTERM');
    setTimeout(() => { try { serverProc.kill('SIGKILL'); } catch {} }, 2000);
  }
  if (tmpDir) {
    try { rmSync(tmpDir, { recursive: true, force: true }); } catch {}
  }
}

// ── REST helpers ────────────────────────────────────────────────────────
async function rest(method, path, body) {
  const res = await fetch(BASE + path, {
    method, headers: { 'Content-Type': 'application/json' },
    body: body ? JSON.stringify(body) : undefined,
  });
  const text = await res.text();
  let json = null;
  try { json = text ? JSON.parse(text) : null; } catch {}
  if (!res.ok) throw new Error(`${method} ${path} → ${res.status}: ${text}`);
  return json;
}

// ── Mappa rotte ─────────────────────────────────────────────────────────
const routes = new Map();
const mapId = JSON.parse(readFileSync(join(ROOT, 'data', 'rulesets', `${RULESET}.json`), 'utf8')).map;
const mapPath = join(ROOT, 'data', 'maps', `${mapId}.json`);
for (const r of JSON.parse(readFileSync(mapPath, 'utf8')).routes) routes.set(r.id, r);
const toCardColor = (c) => (c === 'Gray' ? null : c.toUpperCase());

// ── Player handle ───────────────────────────────────────────────────────
function makePlayer(name, playerId, token, gameId) {
  const ws = new WebSocket(`${WS_BASE}/ws/${gameId}?playerId=${playerId}&token=${token}`);
  const p = {
    name, ws, seq: 0,
    playerId, token, gameId,
    private: null, public: null, offered: null,
    errors: [], snaps: 0, waiters: [],
    // Track last raw message for dump on failure
    lastRaw: null,
  };
  ws.addEventListener('message', (evt) => {
    const raw = typeof evt.data === 'string' ? evt.data : '(binary)';
    p.lastRaw = raw;
    let msg;
    try { msg = JSON.parse(raw); } catch { return; }
    if (msg.type === 'SNAPSHOT') {
      p.snaps++;
      p.public = msg.payload.public;
      if (msg.payload.private) {
        p.private = msg.payload.private;
        if (p.private.offered_tickets?.length) p.offered = p.private.offered_tickets;
      }
    } else if (msg.type === 'PUBLIC_STATE') p.public = msg.payload;
    else if (msg.type === 'PRIVATE_STATE') {
      p.private = msg.payload;
      if (p.private.offered_tickets?.length) p.offered = p.private.offered_tickets;
    } else if (msg.type === 'ERROR') p.errors.push(msg.payload);

    for (const w of p.waiters) w();
    p.waiters = [];
  });
  ws.addEventListener('error', () => {});

  p.wait = (pred, timeout = 15000) => new Promise((resolve, reject) => {
    if (pred(p)) return resolve();
    const t = setTimeout(() => reject(new Error(`${name}: timeout ${timeout}ms`)), timeout);
    const check = () => { if (pred(p)) { clearTimeout(t); resolve(); } };
    p.waiters.push(check);
    const iv = setInterval(() => { check(); if (pred(p)) clearInterval(iv); }, 100);
    setTimeout(() => clearInterval(iv), timeout);
  });

  p.send = (action) => { p.seq += 1; ws.send(JSON.stringify({ ...action, player_id: playerId, client_seq: p.seq })); };
  return p;
}

// ── Bot helpers ─────────────────────────────────────────────────────────
function countInHand(hand, color) { return hand.filter((h) => h === color).length; }

// Le rotte traghetto (`ferries`) richiedono almeno quelle locomotive nel pagamento.
const COLORS = ['RED', 'ORANGE', 'YELLOW', 'GREEN', 'BLUE', 'PURPLE', 'BLACK', 'WHITE'];

// Su un tunnel (Europe) il bot tenta solo con `spare` carte oltre la lunghezza
// (colore o locomotive), così può pagare il costo extra rivelato.
function canPay(route, hand, spare = 0) {
  const locos = countInHand(hand, 'LOCOMOTIVE');
  if (locos < (route.ferries || 0)) return false;
  const need = route.len + (EUROPE && route.tunnel ? spare : 0);
  const rc = toCardColor(route.color);
  if (rc) return countInHand(hand, rc) + locos >= need;
  const bestN = COLORS.reduce((max, c) => Math.max(max, countInHand(hand, c)), 0);
  return bestN + locos >= need;
}

function buildPayment(route, hand) {
  const rc = toCardColor(route.color);
  const color = rc ?? COLORS.map((c) => ({ c, n: countInHand(hand, c) })).sort((a, b) => b.n - a.n)[0].c;
  const n = Math.min(countInHand(hand, color), route.len - (route.ferries || 0));
  return [...Array(n).fill(color), ...Array(route.len - n).fill('LOCOMOTIVE')];
}

// ── State helpers ───────────────────────────────────────────────────────
/** Ritorna lo stato più recente visto dal giocatore, o da p1/p2 come fallback. */
function latestPublic(cur, p1, p2) {
  return cur.public ?? p1.public ?? p2.public;
}

/** Verifica che il giocatore sia nella fase attesa prima di agire. */
function phaseOk(cur, p1, p2, expected) {
  const pub = latestPublic(cur, p1, p2);
  if (!pub) return false;
  // LAST_ROUND is a sub-state: the game is still in PLAYER_TURN but last_round flag is set
  if (expected === 'PLAYER_TURN') return pub.state === 'PLAYER_TURN' || pub.state === 'LAST_ROUND';
  return pub.state === expected;
}

// ── Main ────────────────────────────────────────────────────────────────
async function main() {
  process.on('exit', cleanup);
  process.on('SIGINT', () => { cleanup(); process.exit(1); });
  process.on('SIGTERM', () => { cleanup(); process.exit(1); });

  await startServer();

  console.log('=== CiufCiuf! Full-Game E2E Test ===\n');

  // 1. Crea partita
  console.log('1. Creazione partita (seed=' + SEED + ')...');
  const game = await rest('POST', '/api/games', { rulesetId: RULESET, name: 'FullGameTest', seed: SEED });
  const { gameId, hostToken } = game;
  console.log(`   gameId=${gameId}`);

  // 2. Join 2 giocatori
  console.log('2. Join P1 (RED) e P2 (BLUE)...');
  const p1Join = await rest('POST', `/api/games/${gameId}/join`, { name: 'BotRed', color: 'RED' });
  const p2Join = await rest('POST', `/api/games/${gameId}/join`, { name: 'BotBlue', color: 'BLUE' });
  console.log(`   P1: ${p1Join.playerId}, P2: ${p2Join.playerId}`);

  // 3. Start
  console.log('3. Start...');
  await rest('POST', `/api/games/${gameId}/start`, { hostToken });

  // 4. Connect WS
  console.log('4. Connessione WebSocket...');
  const p1 = makePlayer('BotRed', p1Join.playerId, p1Join.token, gameId);
  const p2 = makePlayer('BotBlue', p2Join.playerId, p2Join.token, gameId);
  const byId = { [p1Join.playerId]: p1, [p2Join.playerId]: p2 };

  await sleep(1000);
  assert(p1.snaps >= 1, 'P1: nessuno SNAPSHOT iniziale');
  assert(p2.snaps >= 1, 'P2: nessuno SNAPSHOT iniziale');
  console.log('   Entrambi connessi');

  // 5. Draft iniziale (accetta tutto)
  console.log('5. Draft iniziale...');
  for (const p of [p1, p2]) {
    await p.wait((x) => x.offered && x.offered.length >= 3, 20000);
    console.log(`   ${p.name}: ${p.offered.length} biglietti → tiene tutti`);
    p.send({ actionType: 'CHOOSE_TICKETS', keep: p.offered.map((t) => t.id) });
  }
  await p1.wait((x) => x.public && x.public.state === 'PLAYER_TURN', 15000);
  console.log('   Draft completato → PLAYER_TURN');

  // ── Stats ─────────────────────────────────────────────────────────────
  let turns = 0;
  let claimsP1 = 0, claimsP2 = 0;
  let stationsP1 = 0, stationsP2 = 0, tunnelsPaid = 0, tunnelsDeclined = 0;
  let drawsP1 = 0, drawsP2 = 0;
  let ticketDrawsP1 = 0, ticketDrawsP2 = 0;
  let lastRoundTriggered = false, lastRoundPlayer = null, lastRoundTriggerPlayer = null;
  const failedClaims = new Set();       // per-player: "playerId:routeId"
  let consecutiveFailures = 0;

  console.log('\n── Partita in corso ──');

  // 6. Main game loop
  while (turns < MAX_TURNS) {
    // Use the most recent public state. Compare both players' views.
    // If they disagree on current player, prefer the one whose
    // current_turn_player_id matches a known player.
    const pp1 = p1.public, pp2 = p2.public;
    let pub = null;
    if (pp1 && pp2) {
      const c1 = pp1.current_turn_player_id;
      const c2 = pp2.current_turn_player_id;
      if (c1 && c2 && c1 !== c2) {
        // States disagree — wait briefly for the lagging connection to catch up
        await sleep(200);
        pub = p1.public ?? p2.public;
      } else {
        pub = pp1; // they agree, or one is null — use either
      }
    } else {
      pub = pp1 ?? pp2;
    }
    if (!pub) { await sleep(200); continue; }

    const state = pub.state;
    if (state === 'GAME_OVER') { console.log('\n   🏁 GAME_OVER!'); break; }

    const curId = pub.current_turn_player_id;
    if (!curId) { await sleep(200); continue; }

    const cur = byId[curId];
    if (!cur) { await sleep(200); continue; }

    // Last round tracking
    if (pub.last_round && !lastRoundTriggered) {
      lastRoundTriggered = true;
      lastRoundPlayer = cur.name;
      lastRoundTriggerPlayer = byId[pub.last_round_trigger_player_id]?.name || '?';
      console.log(`\n   ⚡ ULTIMO GIRO! (trigger: ${lastRoundTriggerPlayer})`);
    }

    // Wait for private state with legal_moves for THIS player
    try {
      await cur.wait((x) => x.private && x.private.legal_moves, 15000);
    } catch {
      // Timeout — maybe game ended or state changed
      turns++;
      continue;
    }

    // Let any pending PUBLIC_STATE arrive before reading phase
    await sleep(50);

    // Re-read state from the CURRENT player's public (most reliable after waiter)
    const curPub = cur.public;
    const curState = curPub?.state;

    // ── DRAFT (mid-game) ──
    if (curState === 'DRAFT' || curState === 'TICKET_CHOICE' || curState === 'INITIAL_DRAFT') {
      const errBefore = cur.errors.length;
      try {
        await cur.wait((x) => x.offered && x.offered.length >= 1, 10000);
      } catch {
        // Timeout — maybe deck exhausted, check if offered is available (even empty)
        if (cur.offered && cur.offered.length === 0) {
          console.log(`   [T${turns}] ${cur.name} DRAFT: nessun biglietto offerto → skip`);
          cur.send({ actionType: 'CHOOSE_TICKETS', keep: [] });
        } else {
          console.log(`   [T${turns}] ${cur.name} DRAFT: timeout aspettando offered tickets`);
        }
        turns++;
        continue;
      }
      if (cur.errors.length > errBefore) {
        console.log(`   [T${turns}] ${cur.name} DRAFT error: ${cur.errors[cur.errors.length-1]?.reason}`);
        turns++;
        continue;
      }
      const nOffered = cur.offered?.length || 0;
      console.log(`   [T${turns}] ${cur.name} DRAFT: ${nOffered} tickets → keep all`);
      cur.send({ actionType: 'CHOOSE_TICKETS', keep: (cur.offered || []).map((t) => t.id) });
      if (cur === p1) ticketDrawsP1++; else ticketDrawsP2++;
      try {
        await cur.wait((x) => {
            const st = (latestPublic(cur, p1, p2) ?? pub).state;
            return st !== 'DRAFT' && st !== 'TICKET_CHOICE' && st !== 'INITIAL_DRAFT';
          }, 15000);
      } catch {}
      turns++;
      continue;
    }

    // ── SECOND_CARD_DRAW ──
    if (curState === 'SECOND_CARD_DRAW') {
      if (cur.public?.state !== 'SECOND_CARD_DRAW') {
        console.log(`   [T${turns}] ${cur.name}: expected SECOND_CARD_DRAW but cur.public.state=${cur.public?.state}, skipping`);
        await sleep(200);
        turns++;
        continue;
      }
      console.log(`   [T${turns}] ${cur.name} 2nd draw: deck`);
      cur.send({ actionType: 'DRAW_TRAIN_CARD', source: 'deck' });
      if (cur === p1) drawsP1++; else drawsP2++;
      consecutiveFailures = 0;
      try {
        await cur.wait((x) => (latestPublic(cur, p1, p2) ?? pub).state !== 'SECOND_CARD_DRAW', 15000);
      } catch {}
      turns++;
      continue;
    }

    // ── PLAYER_TURN (or LAST_ROUND) ──
    if (curState === 'PLAYER_TURN' || curState === 'LAST_ROUND') {
      // Double-check phase with fresh read before any action
      if (cur.public?.state !== 'PLAYER_TURN' && cur.public?.state !== 'LAST_ROUND') {
        console.log(`   [T${turns}] ${cur.name}: phase mismatch (cur.public.state=${cur.public?.state}), re-reading...`);
        await sleep(200);
        turns++;
        continue;
      }

      const lm = cur.private.legal_moves;
      const hand = cur.private.hand;

      if (!lm || !hand) {
        console.log(`   [T${turns}] ${cur.name}: missing legal_moves or hand, waiting...`);
        await sleep(500);
        turns++;
        continue;
      }

      // ── 0. Stazione (Europe): ogni tanto, finché ne restano ──
      if (EUROPE && (lm.station_cities || []).length > 0 && lm.station_cost > 0 && turns % 4 === 0) {
        const city = lm.station_cities[turns % lm.station_cities.length];
        const cost = lm.station_cost;
        const best = COLORS.map((c) => ({ c, n: countInHand(hand, c) })).sort((a, b) => b.n - a.n)[0];
        const n = Math.min(best.n, cost);
        const payment = [...Array(n).fill(best.c), ...Array(cost - n).fill('LOCOMOTIVE')];
        const stationsBefore = cur.public?.stations?.length || 0;
        const errBefore = cur.errors.length;
        console.log(`   [T${turns}] ${cur.name} BUILD_STATION ${city} (costo ${cost}) [${payment.join(',')}]`);
        cur.send({ actionType: 'BUILD_STATION', city_id: city, cards_discarded: payment });
        try {
          await cur.wait((x) => (x.public?.stations?.length || 0) > stationsBefore || x.errors.length > errBefore, 8000);
        } catch {}
        if (cur.errors.length === errBefore) {
          if (cur === p1) stationsP1++; else stationsP2++;
          consecutiveFailures = 0;
          try {
            await cur.wait(() => {
              const cp = latestPublic(cur, p1, p2);
              return !cp || cp.current_turn_player_id !== cur.playerId || cp.state === 'GAME_OVER';
            }, 10000);
          } catch {}
        }
        turns++;
        continue;
      }

      const claimable = (lm.routes || [])
        .filter((rid) => !failedClaims.has(`${cur.playerId}:${rid}`))
        .map((rid) => routes.get(rid))
        .filter((r) => r && canPay(r, hand, turns % 3 === 0 ? 2 : 1))
        // I tunnel pagabili con le carte di scorta passano avanti: sono rari e il test
        // deve esercitare il pagamento del costo extra.
        .sort((a, b) => Number(!!b.tunnel) - Number(!!a.tunnel) || b.len - a.len);

      if (claimable.length > 0) {
        const route = claimable[0];
        const payment = buildPayment(route, hand);
        const errBefore = cur.errors.length;

        // Final phase check right before sending
        const prePub = latestPublic(cur, p1, p2);
        if (prePub?.state !== 'PLAYER_TURN' && prePub?.state !== 'LAST_ROUND') {
          console.log(`   [T${turns}] ${cur.name}: phase changed to ${prePub?.state} before claim, aborting`);
          turns++;
          continue;
        }

        console.log(`   [T${turns}] ${cur.name} CLAIM ${route.id} (len=${route.len}) [${payment.join(',')}]`);
        cur.send({ actionType: 'CLAIM_ROUTE', route_id: route.id, cards_discarded: payment });

        // Wait for explicit confirmation via cur.wait() — 8s timeout.
        // cur.wait() already polls every 100ms via setInterval + triggers on WS messages.
        let claimed = false;
        let claimError = null;
        let claimTimedOut = false;
        let tunnelPending = false;
        const claimErrBefore = cur.errors.length;
        try {
          await cur.wait((x) => {
            // 1) Route explicitly appears in claimed_routes for this player
            if (x.public?.claimed_routes?.some(
              (r) => r.route_id === route.id && r.claimed_by_player_id === cur.playerId
            )) {
              claimed = true;
              return true;
            }
            // 1b) Tunnel rivelato: serve la decisione del giocatore
            if (x.public?.state === 'TUNNEL_RESOLUTION' && x.public.tunnel_reveal?.route_id === route.id) {
              tunnelPending = true;
              return true;
            }
            // 2) Server sent an ERROR
            if (x.errors.length > claimErrBefore) {
              claimError = x.errors[x.errors.length - 1];
              return true;
            }
            // 3) Turn changed away from us (claim succeeded)
            const cp = latestPublic(cur, p1, p2);
            if (cp?.current_turn_player_id !== cur.playerId) {
              return true;
            }
            return false;
          }, 8000);
        } catch {
          claimTimedOut = true;
        }

        // Tunnel: la rotta resta in attesa della decisione (TUNNEL_RESOLUTION).
        if (tunnelPending) {
          console.log(`     → tunnel in attesa della decisione`);
          turns++;
          continue;
        }

        // Post-wait decision
        if (claimTimedOut) {
          // Double-check: maybe the route WAS claimed but we missed the message
          const cp = latestPublic(cur, p1, p2);
          if (cp?.claimed_routes?.some(
            (r) => r.route_id === route.id && r.claimed_by_player_id === cur.playerId
          )) {
            claimed = true;
          } else if (cp?.current_turn_player_id !== cur.playerId) {
            // Turn changed — claim likely succeeded even though we didn't see the confirmation
            claimed = true;
          } else {
            // True timeout — no response from server in 8s
            const dumpPub = JSON.stringify(cp ?? {}, null, 2);
            const dumpPriv = JSON.stringify(cur.private ?? {}, null, 2);
            const lastMsg = cur.lastRaw || '(nessun messaggio ricevuto)';
            fail(
              `Claim ${route.id} timeout (8s senza risposta dal server)!\n` +
              `  Player: ${cur.name} (${cur.playerId})\n` +
              `  Last raw message: ${lastMsg.slice(0, 500)}\n` +
              `  Public state: ${dumpPub.slice(0, 1000)}\n` +
              `  Private: ${dumpPriv.slice(0, 1000)}`
            );
          }
        }

        if (claimed) {
          if (cur === p1) claimsP1++; else claimsP2++;
          failedClaims.clear();
          consecutiveFailures = 0;
          console.log(`     → OK`);
          // Wait for turn to be fully processed
          try {
            await cur.wait((x) => {
              const cp = latestPublic(cur, p1, p2);
              return !cp || cp.current_turn_player_id !== cur.playerId || cp.state === 'GAME_OVER';
            }, 10000);
          } catch {}
          turns++;
          continue;
        }

        if (claimError) {
          console.log(`     → ERROR: ${claimError.reason || 'unknown'}`);
          failedClaims.add(`${cur.playerId}:${route.id}`);
          consecutiveFailures++;
        } else {
          // No error and not claimed — silent rejection (wrong phase, etc.)
          const dbgPub = JSON.stringify(latestPublic(cur, p1, p2) ?? {});
          console.log(`     → silently rejected (state=${dbgPub.slice(0,200)}, errs=${cur.errors.length - claimErrBefore}, lastRaw=${(cur.lastRaw||'').slice(0,150)})`);
          failedClaims.add(`${cur.playerId}:${route.id}`);
          consecutiveFailures++;
        }

        if (consecutiveFailures > 3) {
          console.log(`   [T${turns}] ${cur.name}: troppi claim falliti (${consecutiveFailures}), passo a draw`);
          // Force a draw instead of retrying more claims
          if (lm.can_draw_deck) {
            console.log(`     → draw deck forzato (1st draw)`);
            cur.send({ actionType: 'DRAW_TRAIN_CARD', source: 'deck' });
            if (cur === p1) drawsP1++; else drawsP2++;
            consecutiveFailures = 0;
            // Wait for SECOND_CARD_DRAW phase
            await sleep(300);
            try {
              await cur.wait((x) => {
                const cp = latestPublic(cur, p1, p2);
                return cp && cp.state === 'SECOND_CARD_DRAW';
              }, 5000);
            } catch {
              console.log(`     → 2nd draw timeout, skipping`);
              turns++;
              continue;
            }
            // Now do the second draw (we're in SECOND_CARD_DRAW)
            console.log(`     → 2nd draw (deck)`);
            cur.send({ actionType: 'DRAW_TRAIN_CARD', source: 'deck' });
            if (cur === p1) drawsP1++; else drawsP2++;
            // Wait for turn to pass after second draw
            try {
              await cur.wait((x) => {
                const cp = latestPublic(cur, p1, p2);
                return !cp || cp.current_turn_player_id !== cur.playerId || cp.state === 'GAME_OVER';
              }, 10000);
            } catch {}
          } else {
            console.log(`     → nessuna azione disponibile, skip`);
            consecutiveFailures = 0;
            await sleep(500);
          }
          turns++;
          continue;
        }

        // Still our turn? Let the loop re-evaluate — another route may work
        const curPubNow = latestPublic(cur, p1, p2);
        if (curPubNow?.current_turn_player_id === cur.playerId &&
            (curPubNow?.state === 'PLAYER_TURN' || curPubNow?.state === 'LAST_ROUND')) {
          continue; // back to top of while loop, re-evaluates state
        }
        turns++;
        continue;
      }

      // ── 2. Draw tickets ──
      const myTicketDraws = cur === p1 ? ticketDrawsP1 : ticketDrawsP2;
      if (lm.can_draw_tickets && myTicketDraws < 6) {
        // Re-verify phase
        const dpub = latestPublic(cur, p1, p2);
        if (dpub?.state !== 'PLAYER_TURN' && dpub?.state !== 'LAST_ROUND') {
          console.log(`   [T${turns}] ${cur.name}: phase changed before draw tickets (${dpub?.state}), skipping`);
          turns++;
          continue;
        }

        const errBefore = cur.errors.length;
        console.log(`   [T${turns}] ${cur.name} DRAW_TICKETS`);
        cur.send({ actionType: 'DRAW_TICKETS' });
        try {
          await cur.wait((x) => {
            const cp = latestPublic(cur, p1, p2);
            return cp?.state === 'TICKET_CHOICE' || x.errors.length > errBefore;
          }, 8000);
        } catch {
          console.log(`     → DRAW_TICKETS: nessuna risposta in 8s, skip`);
          turns++;
          continue;
        }
        if (cur.errors.length > errBefore) {
          console.log(`     → DRAW_TICKETS error: ${cur.errors[cur.errors.length-1]?.reason}`);
          turns++;
          continue;
        }
        if (cur === p1) ticketDrawsP1++; else ticketDrawsP2++;
        consecutiveFailures = 0;
        // Wait for offered tickets, then DRAFT will be handled next iteration
        try {
          await cur.wait((x) => x.offered && x.offered.length >= 1, 10000);
        } catch {}
        continue; // DRAFT handled at top of loop
      }

      // ── 3. Draw train card ──
      let drew = false;

      // Prefer deck if available (and phase is correct)
      if (lm.can_draw_deck) {
        const dpub = latestPublic(cur, p1, p2);
        if (dpub?.state === 'PLAYER_TURN' || dpub?.state === 'LAST_ROUND') {
          console.log(`   [T${turns}] ${cur.name} DRAW deck`);
          cur.send({ actionType: 'DRAW_TRAIN_CARD', source: 'deck' });
          drew = true;
        }
      }

      if (!drew && (lm.draw_face_up || []).length > 0) {
        const dpub = latestPublic(cur, p1, p2);
        if (dpub?.state === 'PLAYER_TURN' || dpub?.state === 'LAST_ROUND') {
          const vis = dpub?.visible_deck || [];
          const idx = lm.draw_face_up.find((i) => vis[i] !== 'LOCOMOTIVE');
          const fi = idx !== undefined ? idx : lm.draw_face_up[0];
          console.log(`   [T${turns}] ${cur.name} DRAW faceUp[${fi}]`);
          cur.send({ actionType: 'DRAW_TRAIN_CARD', source: { faceUp: { index: fi } } });
          drew = true;
        }
      }

      if (drew) {
        if (cur === p1) drawsP1++; else drawsP2++;
        failedClaims.clear();
        consecutiveFailures = 0;
        try {
          await cur.wait((x) => {
            const cp = latestPublic(cur, p1, p2);
            const sid = cp?.current_turn_player_id;
            const st = cp?.state;
            return (st !== 'PLAYER_TURN' && st !== 'LAST_ROUND') || sid !== cur.playerId || st === 'GAME_OVER';
          }, 15000);
        } catch {}
        turns++;
        continue;
      }

      // Nothing actionable — shouldn't happen but handle gracefully
      console.log(`   [T${turns}] ${cur.name}: BLOCCATO — routes=${(lm.routes||[]).length} canDrawTickets=${lm.can_draw_tickets} canDrawDeck=${lm.can_draw_deck} faceUp=${(lm.draw_face_up||[]).length} failedClaims=${[...failedClaims].filter(s => s.startsWith(cur.playerId)).length}`);
      await sleep(500);
      turns++;
      continue;
    }


    // ── TUNNEL_RESOLUTION (Europe): paga se può, altrimenti rinuncia ──
    if (curState === 'TUNNEL_RESOLUTION') {
      const tunnel = cur.private?.legal_moves?.tunnel;
      if (!tunnel) {
        await sleep(200);
        turns++;
        continue;
      }
      assert(
        cur.public.tunnel_reveal && cur.public.tunnel_reveal.route_id === tunnel.route_id,
        'tunnel_reveal assente o diverso da legal_moves.tunnel',
      );
      const hand = cur.private.hand;
      const errBefore = cur.errors.length;
      let pay = tunnel.can_pay;
      let cards = [];
      if (pay) {
        const same = hand.filter((c) => c === tunnel.color).slice(0, tunnel.extra_cost);
        const locos = hand.filter((c) => c === 'LOCOMOTIVE' && tunnel.color !== 'LOCOMOTIVE');
        cards = [...same, ...locos.slice(0, tunnel.extra_cost - same.length)];
        if (cards.length !== tunnel.extra_cost) pay = false;
      }
      console.log(`   [T${turns}] ${cur.name} TUNNEL ${tunnel.route_id} extra=${tunnel.extra_cost} (${tunnel.color}) → ${pay ? 'PAGA' : 'RINUNCIA'}`);
      cur.send({ actionType: 'TUNNEL_DECISION', pay, cards_discarded: pay ? cards : [] });
      if (pay) tunnelsPaid++; else tunnelsDeclined++;
      try {
        await cur.wait((x) => {
          const cp = latestPublic(cur, p1, p2);
          return (cp && cp.state !== 'TUNNEL_RESOLUTION') || x.errors.length > errBefore;
        }, 10000);
      } catch {}
      turns++;
      continue;
    }

    // Unknown / transitional state — wait
    await sleep(200);
    turns++;
  }

  if (turns >= MAX_TURNS) {
    const fp = latestPublic(p1, p1, p2) ?? latestPublic(p2, p1, p2);
    const fpState = fp?.state || '?';
    const fpCur = fp?.current_turn_player_id || '?';
    fail(`Limite ${MAX_TURNS} turni raggiunto senza GAME_OVER.\n` +
      `  Ultimo stato: ${fpState}, current_player: ${byId[fpCur]?.name || fpCur}\n` +
      `  Last round triggered: ${lastRoundTriggered ? 'YES' : 'NO'}\n` +
      `  Claims P1=${claimsP1} P2=${claimsP2}  Draws P1=${drawsP1} P2=${drawsP2}\n` +
      `  Ticket draws P1=${ticketDrawsP1} P2=${ticketDrawsP2}\n` +
      `  Routes claimed: ${fp?.claimed_routes?.length || 0}\n` +
      `  Errors P1=${p1.errors.length} P2=${p2.errors.length}\n` +
      `  P1 last raw: ${(p1.lastRaw || '').slice(0, 300)}\n` +
      `  P2 last raw: ${(p2.lastRaw || '').slice(0, 300)}`);
  }

  // ── Verifiche finali ──────────────────────────────────────────────
  console.log('\n── Verifiche finali ──');
  const finalPub = latestPublic(p1, p1, p2) ?? latestPublic(p2, p1, p2);
  assert(finalPub, 'PublicState finale assente');

  const totalErrors = p1.errors.length + p2.errors.length;
  console.log(`   ERROR totali: P1=${p1.errors.length}, P2=${p2.errors.length}`);
  for (const e of p1.errors) console.log(`     P1: ${e.reason} (ref: ${e.action_ref})`);
  for (const e of p2.errors) console.log(`     P2: ${e.reason} (ref: ${e.action_ref})`);
  assert(totalErrors === 0, `${totalErrors} ERROR during game`);

  assert(finalPub.final_scores?.length > 0, 'final_scores missing');
  assert(finalPub.players_summary?.length === 2, `players_summary: ${finalPub.players_summary?.length}`);
  assert(finalPub.longest_path_player_ids?.length > 0, 'longest_path missing');

  // ── Report ─────────────────────────────────────────────────────────
  console.log('\n── Report partita ──');
  console.log(`   Turni totali:        ${turns}`);
  console.log(`   Claims:              P1=${claimsP1}, P2=${claimsP2} (server: ${finalPub.claimed_routes?.length || 0})`);
  console.log(`   Pesca carte:         P1=${drawsP1}, P2=${drawsP2}`);
  console.log(`   Pesca biglietti:     P1=${ticketDrawsP1}, P2=${ticketDrawsP2}`);
  console.log(`   Ultimo giro:         ${lastRoundTriggered ? 'SI' : 'NO'}`);
  if (lastRoundTriggered) {
    console.log(`     Trigger player:    ${lastRoundTriggerPlayer || '?'}`);
    console.log(`     Player corrente:   ${lastRoundPlayer || '?'}`);
    console.log(`     Turno al trigger:  ${turns > 0 ? 'vedi sopra' : '?'}`);
  }

  for (const ps of (finalPub.players_summary || [])) {
    const nm = byId[ps.playerId || ps.player_id]?.name || ps.playerId || ps.player_id || '?';
    console.log(`   ${nm}: score=${ps.score}, trains=${ps.trainsRemaining ?? ps.trains_remaining}, hand=${ps.cardsHandCount ?? ps.cards_hand_count}, tickets=${ps.ticketsCount ?? ps.tickets_count}`);
  }
  console.log(`   Longest path bonus: ${(finalPub.longest_path_player_ids || []).map((pid) => byId[pid]?.name || pid).join(', ')}`);

  if (finalPub.final_scores) {
    console.log('   Punteggi finali:');
    for (const fs of finalPub.final_scores) {
      const nm = byId[fs.playerId]?.name || fs.playerId || '?';
      const borrowed = (fs.borrowedRoutes || []).map((b) => `${b.cityId}:${b.routeId}`).join(',') || '-';
      console.log(`     ${nm}: routes=${fs.routePoints} tickets=${fs.ticketCompleted}/${fs.ticketFailed} longest=${fs.longestPathBonus} stations=${fs.stationPoints} borrowed=${borrowed} → ${fs.total}`);
      assert(Number.isInteger(fs.stationPoints), `${nm}: stationPoints mancante nei punteggi finali`);
      assert(Array.isArray(fs.borrowedRoutes), `${nm}: borrowedRoutes mancante nei punteggi finali`);
      assert(
        fs.total === fs.routePoints + fs.ticketCompleted + fs.ticketFailed + fs.longestPathBonus + fs.stationPoints,
        `${nm}: totale incoerente`,
      );
      if (!EUROPE) assert(fs.stationPoints === 0 && fs.borrowedRoutes.length === 0, `${nm}: stazioni su una mappa senza stazioni`);
    }
  }

  if (EUROPE) {
    console.log(`   Stazioni costruite:  P1=${stationsP1}, P2=${stationsP2} (server: ${finalPub.stations?.length || 0})`);
    console.log(`   Tunnel:              pagati=${tunnelsPaid}, rinunce=${tunnelsDeclined}`);
    assert((finalPub.stations?.length || 0) === stationsP1 + stationsP2, 'stazioni del server diverse da quelle costruite');
    assert(stationsP1 + stationsP2 > 0, 'nessuna stazione costruita');
    assert(tunnelsPaid > 0, 'nessun tunnel pagato');
    for (const fs of finalPub.final_scores) {
      const mine = (finalPub.stations || []).filter((s) => s.player_id === fs.playerId).length;
      assert(fs.stationPoints === (3 - mine) * 4, `${fs.playerId}: stationPoints ${fs.stationPoints} con ${mine} stazioni`);
    }
  }

  // Ultime mosse: presenti e senza carte pescate (nessun campo `card`).
  const recent = finalPub.recent_moves;
  assert(Array.isArray(recent) && recent.length > 0, 'recent_moves vuoto o assente');
  assert(recent.length <= 8, `recent_moves oltre il massimo: ${recent.length}`);
  const hasCardField = (v) => v !== null && typeof v === 'object'
    && (Array.isArray(v) ? v.some(hasCardField) : Object.entries(v).some(([k, x]) => k === 'card' || hasCardField(x)));
  assert(!hasCardField(recent), 'recent_moves espone un campo `card`');
  console.log(`   Ultime mosse:        ${recent.length} (${recent.map((m) => m.kind).join(', ')})`);

  // WS ERROR detail
  if (totalErrors > 0) {
    console.log('\n   ⚠️  ERROR WS riscontrati:');
    for (const e of p1.errors) console.log(`     [P1] ${e.reason} (code=${e.code}, ref=${e.action_ref})`);
    for (const e of p2.errors) console.log(`     [P2] ${e.reason} (code=${e.code}, ref=${e.action_ref})`);
  } else {
    console.log('\n   ✅ Zero ERROR WS');
  }

  p1.ws.close();
  p2.ws.close();
  await sleep(300);

  console.log('\n=== Full-game test OK! ===');
  cleanup();
  process.exit(0);
}

main().catch((e) => {
  console.error('❌ FATAL:', e);
  cleanup();
  process.exit(1);
});