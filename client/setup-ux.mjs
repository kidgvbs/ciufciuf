// Setup partita di prova per la review UX: crea, joina 2 giocatori, start,
// completa il draft iniziale e gioca qualche turno (claim + pesche) fino a
// uno stato mid-game ricco: rotte reclamate da entrambi, turno di Gino con
// rotte claimabili. Stampa alla fine l'identità p1 per localStorage.
//
// Uso: node setup-ux.mjs [base=http://localhost:18940]

const BASE = process.argv[2] ?? 'http://localhost:18940';
const WS_BASE = BASE.replace(/^http/, 'ws');
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const assert = (cond, msg) => { if (!cond) { console.error('❌ ' + msg); process.exit(1); } };

import { readFileSync } from 'node:fs';

const routes = new Map();
for (const r of JSON.parse(readFileSync(new URL('../data/maps/usa.json', import.meta.url))).routes) routes.set(r.id, r);
// data usa "Gray"/"Red" (PascalCase) → CardColor proto (UPPER)
const toCardColor = (c) => (c === 'Gray' ? null : c.toUpperCase());

async function rest(method, path, body) {
  const res = await fetch(BASE + path, {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: body ? JSON.stringify(body) : undefined,
  });
  const text = await res.text();
  let json = null;
  try { json = text ? JSON.parse(text) : null; } catch {}
  if (!res.ok) throw new Error(`${method} ${path} → ${res.status}: ${text}`);
  return json;
}

// Player handle: WS connesso + ultimo stato privato/pubblico noto
function makePlayer(name, playerId, token, gameId) {
  const ws = new WebSocket(`${WS_BASE}/ws/${gameId}?playerId=${playerId}&token=${token}`);
  const p = {
    name, ws, seq: 0,
    private: null,          // ultimo PrivateState
    public: null,           // ultimo PublicState
    offered: null,          // biglietti offerti nel draft
    waiters: [],
  };
  ws.addEventListener('message', (evt) => {
    const msg = JSON.parse(evt.data);
    if (msg.type === 'SNAPSHOT') {
      p.public = msg.payload.public;
      if (msg.payload.private) { p.private = msg.payload.private; if (p.private.offered_tickets?.length) p.offered = p.private.offered_tickets; }
    } else if (msg.type === 'PUBLIC_STATE') p.public = msg.payload;
    else if (msg.type === 'PRIVATE_STATE') {
      p.private = msg.payload;
      if (p.private.offered_tickets?.length) p.offered = p.private.offered_tickets;
    }
    for (const w of p.waiters) w();
    p.waiters = [];
  });
  p.wait = (pred, timeout = 8000) => new Promise((resolve, reject) => {
    if (pred(p)) return resolve();
    const t = setTimeout(() => reject(new Error(`${name}: timeout attesa stato`)), timeout);
    const check = () => { if (pred(p)) { clearTimeout(t); resolve(); } };
    p.waiters.push(check);
    // poll di sicurezza (i waiters scattano solo su messaggi)
    const iv = setInterval(() => { check(); if (pred(p)) clearInterval(iv); }, 100);
    setTimeout(() => clearInterval(iv), timeout);
  });
  p.send = (action) => { p.seq += 1; ws.send(JSON.stringify({ ...action, player_id: playerId, client_seq: p.seq })); };
  return p;
}

async function main() {
  console.log('1. Creazione partita (seed fisso)...');
  const game = await rest('POST', '/api/games', { rulesetId: 'usa_v2', name: 'Serata UX', seed: 20261104 });
  const { gameId, hostToken } = game;
  console.log(`   gameId=${gameId}`);

  console.log('2. Join Gino (RED) e Mila (BLUE)...');
  const g1 = await rest('POST', `/api/games/${gameId}/join`, { name: 'Gino', color: 'RED' });
  const g2 = await rest('POST', `/api/games/${gameId}/join`, { name: 'Mila', color: 'BLUE' });

  console.log('3. Start...');
  await rest('POST', `/api/games/${gameId}/start`, { hostToken });

  const p1 = makePlayer('Gino', g1.playerId, g1.token, gameId);
  const p2 = makePlayer('Mila', g2.playerId, g2.token, gameId);
  await sleep(700);

  console.log('4. Draft iniziale (ognuno tiene i primi 2 biglietti)...');
  for (const p of [p1, p2]) {
    await p.wait((x) => x.offered && x.offered.length >= 3);
    p.send({ actionType: 'CHOOSE_TICKETS', keep: p.offered.slice(0, 2).map((t) => t.id) });
  }
  await p1.wait((x) => x.public && x.public.state === 'PLAYER_TURN', 10000);
  console.log('   Draft completato → PLAYER_TURN');

  const byId = { [g1.playerId]: p1, [g2.playerId]: p2 };
  const claimedBy = new Map();

  console.log('5. Gioco alcuni turni (pesche + claim) fino a stato mid-game...');
  let turns = 0;
  while (turns < 40) {
    const pub = p1.public ?? p2.public;
    const cur = byId[pub.current_turn_player_id];
    assert(cur, 'giocatore di turno noto');
    await cur.wait((x) => x.private && x.private.legal_moves, 8000);

    const lm = cur.private.legal_moves;
    const already = (claimedBy.get(cur.name) ?? 0);
    // Obiettivo: entrambi con ≥1 rotta; poi fermarsi quando è il turno di Gino
    // e Gino ha rotte claimabili (per lo screenshot del controller).
    if (lm.routes.length > 0 && (already > 0 || claimedBy.get('Mila') === undefined || claimedBy.get('Mila') > 0)) {
      if (cur.name === 'Gino' && (claimedBy.get('Mila') ?? 0) >= 1) break; // stato finale desiderato
      if (already >= 1 && cur.name === 'Mila') { turns++; /* Mila pesca, per varietà */ }
      else {
        const routeId = lm.routes[0];
        const route = routes.get(routeId);
        assert(route, `rotta ${routeId} nei dati`);
        const hand = cur.private.hand; // Array<CardColor>
        const count = (c) => hand.filter((h) => h === c).length;
        const locos = count('LOCOMOTIVE');
        let cards = [];
        const rc = toCardColor(route.color);
        if (rc) {
          const n = Math.min(count(rc), route.len);
          cards = Array(n).fill(rc).concat(Array(route.len - n).fill('LOCOMOTIVE'));
        } else {
          const best = ['RED','ORANGE','YELLOW','GREEN','BLUE','PURPLE','BLACK','WHITE']
            .map((c) => ({ c, n: count(c) })).sort((a, b) => b.n - a.n)[0];
          const n = Math.min(best.n, route.len);
          cards = Array(n).fill(best.c).concat(Array(route.len - n).fill('LOCOMOTIVE'));
        }
        if (cards.length !== route.len || cards.filter((c) => c !== 'LOCOMOTIVE').length > 0) {
          cur.send({ actionType: 'CLAIM_ROUTE', route_id: routeId, cards_discarded: cards });
          await cur.wait((x) => x.public?.claimed_routes?.some((cr) => cr.route_id === routeId), 8000);
          claimedBy.set(cur.name, (claimedBy.get(cur.name) ?? 0) + 1);
          console.log(`   ${cur.name} ha reclamato ${routeId} (${route.color}, len ${route.len})`);
          turns++;
          continue;
        }
      }
    }
    // Turno di pesca: due pescate dal mazzo chiudono il turno
    cur.send({ actionType: 'DRAW_TRAIN_CARD', source: 'deck' });
    await cur.wait((x) => x.public && (x.public.state === 'SECOND_CARD_DRAW' || x.public.current_turn_player_id !== cur.private.player_id), 8000);
    if (cur.public.state === 'SECOND_CARD_DRAW') cur.send({ actionType: 'DRAW_TRAIN_CARD', source: 'deck' });
    await cur.wait((x) => x.public && x.public.state === 'PLAYER_TURN' && x.public.current_turn_player_id !== (cur.private.player_id), 8000);
    turns++;
  }

  const pub = p1.public;
  console.log(`\nStato finale: ${pub.state}, turno di ${pub.current_turn_player_id === g1.playerId ? 'Gino' : 'Mila'}`);
  console.log(`Rotte reclamate: ${pub.claimed_routes.length}, Gino mano: ${p1.private.hand.length} carte, claimabili: ${p1.private.legal_moves.routes.length}`);

  console.log('\n=== IDENTITÀ P1 (per localStorage) ===');
  console.log(JSON.stringify({ playerId: g1.playerId, token: g1.token, gameId, host: BASE }));
  console.log('=== IDENTITÀ P2 (per localStorage) ===');
  console.log(JSON.stringify({ playerId: g2.playerId, token: g2.token, gameId, host: BASE }));

  p1.ws.close(); p2.ws.close();
  await sleep(300);
  process.exit(0);
}

main().catch((e) => { console.error('❌', e.message); process.exit(1); });
