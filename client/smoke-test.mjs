// Smoke test end-to-end per il client CiufCiuf! (Fase 3a + 3b).
//
// Testa: creazione partita via REST, join, start, connessione WS,
// ricezione SNAPSHOT, disconnect, reconnect con REQUEST_SNAPSHOT,
// spettatore (N1: senza playerId) con REQUEST_SNAPSHOT, statici SPA.
//
// Uso: bun smoke-test.mjs [host]
// (il server deve essere avviato con --client-dist per i check sui statici
// e con --allow-client-seed perché il test crea la partita con un seed)

const HOST = process.argv[2] || 'http://localhost:18921';
const BASE = HOST.replace(/\/$/, '');
const WS_BASE = BASE.replace(/^http/, 'ws');

let gameId, hostToken, playerId, playerToken;

function rest(path, method, body) {
  const opts = { method, headers: { 'Content-Type': 'application/json' } };
  if (body) opts.body = JSON.stringify(body);
  return fetch(`${BASE}${path}`, opts).then(r => {
    if (!r.ok) return r.text().then(t => { throw new Error(`${r.status}: ${t}`); });
    if (r.status === 204) return null;
    return r.json();
  });
}

async function sleep(ms) { return new Promise(r => setTimeout(r, ms)); }

async function main() {
  console.log('=== CiufCiuf! Smoke Test (Fase 3a) ===\n');

  // 1. Creazione partita con seed deterministico
  console.log('1. Creazione partita...');
  const game = await rest('/api/games', 'POST', { rulesetId: 'usa_v2', name: 'SmokeTest', seed: 42 });
  gameId = game.gameId;
  hostToken = game.hostToken;
  console.log(`   Game ID: ${gameId}, hostToken: ${hostToken}`);

  // 2. Join di due giocatori (minimo 2 per USA)
  console.log('2. Join giocatore 1...');
  const player = await rest(`/api/games/${gameId}/join`, 'POST', { name: 'TestPlayer' });
  playerId = player.playerId;
  playerToken = player.token;
  console.log(`   Player ID: ${playerId}, token: ${playerToken}`);

  console.log('2b. Join giocatore 2...');
  const p2 = await rest(`/api/games/${gameId}/join`, 'POST', { name: 'TestPlayer2' });
  console.log(`   Player ID: ${p2.playerId}`);

  // 3. Start partita
  console.log('3. Start...');
  await rest(`/api/games/${gameId}/start`, 'POST', { hostToken });
  console.log('   OK (204)');

  // 4. Connessione WebSocket
  const wsUrl = `${WS_BASE}/ws/${gameId}?playerId=${playerId}&token=${playerToken}`;
  console.log(`4. Connessione WS: ${wsUrl}`);
  let ws = new WebSocket(wsUrl);
  let snapshotCount = 0;
  let publicStates = 0;
  let privateStates = 0;
  let errors = 0;

  ws.addEventListener('open', () => console.log('   WS aperto'));
  ws.addEventListener('message', (evt) => {
    const msg = JSON.parse(evt.data);
    if (msg.type === 'SNAPSHOT') {
      snapshotCount++;
      console.log(`   SNAPSHOT #${snapshotCount}: state=${msg.payload.public.state}, ` +
        `private=${msg.payload.private ? 'SI' : 'NO (spettatore)'}`);
      if (msg.payload.private) {
        console.log(`     hand=${msg.payload.private.hand.length} carte, ` +
          `offered=${msg.payload.private.offered_tickets.length} biglietti, ` +
          `seq=${msg.payload.private.seq}`);
      }
    } else if (msg.type === 'PUBLIC_STATE') {
      publicStates++;
    } else if (msg.type === 'PRIVATE_STATE') {
      privateStates++;
    } else if (msg.type === 'ERROR') {
      errors++;
      console.log(`   ERROR: ${msg.payload.reason} (ref: ${msg.payload.action_ref})`);
    }
  });
  ws.addEventListener('error', (e) => console.error('   WS ERROR:', e.message));
  ws.addEventListener('close', (e) => console.log(`   WS chiuso: code=${e.code}`));

  // Aspetta lo SNAPSHOT iniziale
  await sleep(1500);
  console.log(`   Ricevuti: ${snapshotCount} SNAPSHOT, ${publicStates} PUBLIC, ${privateStates} PRIVATE, ${errors} ERROR`);

  if (snapshotCount < 1) {
    console.error('\n❌ FAIL: nessun SNAPSHOT ricevuto!');
    process.exit(1);
  }
  // Il SNAPSHOT contiene già il PrivateState (E7: private = Some).
  // PRIVATE_STATE separato arriva solo dopo azioni. Il test è OK.
  console.log('\n✅ Connessione e SNAPSHOT OK (private dentro lo snapshot)');

  // 5. Disconnect
  console.log('5. Disconnessione...');
  ws.close();
  await sleep(500);

  // 6. Reconnect + REQUEST_SNAPSHOT
  console.log('6. Reconnect...');
  ws = new WebSocket(wsUrl);
  let reconnected = false;
  ws.addEventListener('open', async () => {
    console.log('   WS riaperto dopo disconnect');
    // Invia REQUEST_SNAPSHOT
    ws.send(JSON.stringify({
      actionType: 'REQUEST_SNAPSHOT',
      player_id: playerId,
      client_seq: null,
    }));
    console.log('   REQUEST_SNAPSHOT inviato');
    reconnected = true;
  });
  ws.addEventListener('message', (evt) => {
    const msg = JSON.parse(evt.data);
    if (msg.type === 'SNAPSHOT') {
      console.log(`   SNAPSHOT dopo reconnect: state=${msg.payload.public.state}, private=${msg.payload.private ? 'SI' : 'NO'}`);
    }
  });
  ws.addEventListener('error', (e) => console.error('   WS ERROR:', e.message));

  await sleep(1500);
  if (!reconnected) {
    console.error('\n❌ FAIL: reconnect fallito!');
    process.exit(1);
  }
  console.log('\n✅ Reconnect OK');
  ws.close();

  // 7. Spettatore (N1): connessione senza playerId né token
  console.log('7. Connessione spettatore (senza playerId)...');
  const specWs = new WebSocket(`${WS_BASE}/ws/${gameId}`);
  let specSnapshot = 0;
  let specPrivate = false;
  let specError = 0;
  specWs.addEventListener('message', (evt) => {
    const msg = JSON.parse(evt.data);
    if (msg.type === 'SNAPSHOT') {
      specSnapshot++;
      if (msg.payload.private) specPrivate = true;
      console.log(`   SNAPSHOT spettatore #${specSnapshot}: state=${msg.payload.public.state}, private=${msg.payload.private ? 'SI' : 'NO'}`);
    } else if (msg.type === 'ERROR') {
      specError++;
      console.log(`   ERROR spettatore: ${msg.payload.reason}`);
    }
  });
  specWs.addEventListener('error', (e) => console.error('   WS spettatore ERROR:', e.message));

  await sleep(1500);
  if (specSnapshot < 1) {
    console.error('\n❌ FAIL: spettatore non ha ricevuto SNAPSHOT!');
    process.exit(1);
  }
  if (specPrivate) {
    console.error('\n❌ FAIL: spettatore ha ricevuto stato privato!');
    process.exit(1);
  }
  console.log('   ✅ SNAPSHOT spettatore OK (solo pubblico)');

  // 8. Spettatore REQUEST_SNAPSHOT (senza player_id, come il client TV)
  console.log('8. Spettatore REQUEST_SNAPSHOT...');
  specWs.send(JSON.stringify({ actionType: 'REQUEST_SNAPSHOT', client_seq: null }));
  await sleep(1000);
  if (specSnapshot < 2) {
    console.error('\n❌ FAIL: spettatore non ha ricevuto SNAPSHOT su richiesta!');
    process.exit(1);
  }
  if (specError > 0) {
    console.error('\n❌ FAIL: spettatore ha ricevuto ERROR su REQUEST_SNAPSHOT!');
    process.exit(1);
  }
  console.log('   ✅ REQUEST_SNAPSHOT spettatore OK');
  specWs.close();

  // 9. Statici SPA (Fase 3b): / e /controller servono index.html
  console.log('9. Statici SPA...');
  for (const path of ['/', '/controller']) {
    const resp = await fetch(`${BASE}${path}`);
    if (resp.status !== 200) {
      console.error(`\n❌ FAIL: GET ${path} → ${resp.status} (avvia il server con --client-dist)`);
      process.exit(1);
    }
    const body = await resp.text();
    if (!body.toLowerCase().includes('<html')) {
      console.error(`\n❌ FAIL: GET ${path} non restituisce HTML`);
      process.exit(1);
    }
    console.log(`   GET ${path} → 200 HTML ✅`);
  }

  // 10. Anti-loop reconnect (P0 Fase 3c): due connessioni sequenziali con la
  // stessa identità → la seconda riceve esattamente 1 SNAPSHOT, niente storm
  // di ERROR né chiusure ripetute.
  console.log('10. Reconnect stessa identità (anti-loop P0)...');
  let snap10 = 0;
  let errors10 = 0;
  for (let attempt = 1; attempt <= 2; attempt++) {
    const w = new WebSocket(wsUrl);
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('timeout apertura WS')), 5000);
      w.addEventListener('open', () => {
        clearTimeout(timer);
        console.log(`   Tentativo ${attempt}: WS aperto`);
        resolve(null);
      });
      w.addEventListener('error', (e) => {
        clearTimeout(timer);
        reject(new Error(`WS error tentativo ${attempt}: ${e.message}`));
      });
    });
    w.addEventListener('message', (evt) => {
      const msg = JSON.parse(evt.data);
      if (msg.type === 'SNAPSHOT') snap10++;
      else if (msg.type === 'ERROR') { errors10++; console.log(`   ERROR: ${msg.payload.reason}`); }
    });
    await sleep(800);
    w.close();
    await sleep(300);
  }
  if (snap10 !== 2) {
    console.error(`\n❌ FAIL: attesi 2 SNAPSHOT (uno per connessione), ricevuti ${snap10}!`);
    process.exit(1);
  }
  if (errors10 > 0) {
    console.error(`\n❌ FAIL: ${errors10} ERROR durante i reconnect (possibile storm)!`);
    process.exit(1);
  }
  console.log('   ✅ 2 reconnect puliti: 1 SNAPSHOT ciascuno, 0 ERROR');

  console.log('\n=== Smoke test completato con successo! ===');
  process.exit(0);
}

main().catch(e => {
  console.error('FATAL:', e);
  process.exit(1);
});