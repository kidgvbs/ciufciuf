<script lang="ts">
  // Punteggi finali (controller, schermata di fine partita): vincitori, classifica ordinata con il
  // dettaglio di ogni giocatore (rotte, biglietti, bonus percorso più lungo, stazioni, rotte in
  // prestito) e chi ha il bonus del percorso più lungo. Solo dati del server.
  import type { Snippet } from 'svelte';
  import { PLAYER_COLOR_CSS } from '../colors';
  import { rankedScores, winnerNames, borrowedRouteLabel, ticketResultLabel } from '../scores';
  import type { MapData } from '../map-utils';
  import type { FinalScoreInfo, PlayerSummary } from '../proto';

  let { scores, players, playerId, longestPathPlayerIds, longestPathName, globetrotterName, mandalaName, mapData, children }: {
    scores: FinalScoreInfo[];
    players: PlayerSummary[];
    /** Giocatore del telefono: la sua scheda è evidenziata. */
    playerId: string;
    /** Chi ha il bonus del percorso più lungo (`null` finché non è noto). */
    longestPathPlayerIds: string[] | null;
    /** Nome del bonus per il percorso più lungo (varia per mappa); `null` se il ruleset non lo prevede. */
    longestPathName: string | null;
    /** Nome del bonus Globetrotter; `null` se il ruleset non lo prevede. */
    globetrotterName: string | null;
    /** Nome del bonus Mandala (Grand Tour); `null` se il ruleset non lo prevede. */
    mandalaName: string | null;
    /** Mappa della partita (nomi delle città); `null` finché non è caricata. */
    mapData: MapData | null;
    /** Contenuto mostrato fra la riga del vincitore e la classifica (es. la mappa). */
    children?: Snippet;
  } = $props();

  const winners = $derived(winnerNames(scores, players));
  const globetrotterIds = $derived(scores.filter((fs) => fs.globetrotterBonus > 0).map((fs) => fs.playerId));

  const mandalaIds = $derived(scores.filter((fs) => fs.mandalaBonus > 0).map((fs) => fs.playerId));

  function namesOf(ids: string[]): string {
    return ids.map((pid) => players.find((ps) => ps.player_id === pid)?.name ?? pid).join(', ');
  }
</script>

{#if winners}<p class="winner-line">🥇 Vince {winners}!</p>{/if}
{@render children?.()}
<div class="scores-list">
  {#each rankedScores(scores) as fs}
    {@const p = players.find((ps) => ps.player_id === fs.playerId)}
    {@const borrowed = fs.borrowedRoutes ?? []}
    <div class="score-card" class:mine={fs.playerId === playerId} class:winner={fs.winner}>
      <div class="score-header">
        <span class="chip-dot" style="background:{p ? PLAYER_COLOR_CSS[p.color] : '#666'}"></span>
        <span class="score-name">{fs.winner ? '🥇 ' : ''}{p?.name ?? fs.playerId}</span>
        <span class="score-total">{fs.total}</span>
      </div>
      <div class="score-detail">
        Rotte {fs.routePoints} · Biglietti +{fs.ticketCompleted}
        {#if fs.ticketFailed < 0} · {fs.ticketFailed}{/if}
        {#if longestPathName !== null && fs.longestPathBonus > 0} · 🏆 +{fs.longestPathBonus}{/if}
        {#if fs.globetrotterBonus > 0} · 🌍 +{fs.globetrotterBonus}{/if}
        {#if (fs.mandalaBonus ?? 0) > 0} · 🪷 +{fs.mandalaBonus}{/if}
        {#if (fs.stationPoints ?? 0) > 0} · 🏠 +{fs.stationPoints}{/if}
        {#if (fs.countryPoints ?? 0) > 0} · Carte Paese +{fs.countryPoints}{/if}
        {#if (fs.passengerPoints ?? 0) > 0} · Passeggeri +{fs.passengerPoints}{/if}
      </div>
      {#if borrowed.length > 0}
        <div class="score-detail">
          Rotte in prestito:
          {borrowed.map((b) => borrowedRouteLabel(b, mapData)).join(', ')}
        </div>
      {/if}
      {#if (fs.tickets ?? []).length > 0}
        <details class="ticket-results">
          <summary>Biglietti ({fs.tickets.length})</summary>
          <ul>
            {#each fs.tickets as r (r.ticketId)}
              <li>{ticketResultLabel(r, mapData)}</li>
            {/each}
          </ul>
        </details>
      {/if}
    </div>
  {/each}
</div>
{#if longestPathName !== null && longestPathPlayerIds}
  <div class="longest-banner">
    🏆 {longestPathName}: {namesOf(longestPathPlayerIds)}
  </div>
{/if}
{#if globetrotterName !== null && globetrotterIds.length > 0}
  <div class="longest-banner">
    🌍 {globetrotterName}: {namesOf(globetrotterIds)}
  </div>
{/if}
{#if mandalaName !== null && mandalaIds.length > 0}
  <div class="longest-banner">
    🪷 {mandalaName}: {namesOf(mandalaIds)}
  </div>
{/if}

<style>
  .chip-dot {
    display: inline-block;
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  .winner-line {
    font-size: 1.2rem;
    font-weight: 800;
    color: var(--gold);
    margin: 0 0 0.75rem;
  }
  .scores-list {
    width: 100%;
    max-width: 380px;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .score-card {
    background: var(--chrome-bg-1);
    border: 1px solid var(--chrome-line);
    border-radius: 8px;
    box-shadow: var(--shadow-panel);
    padding: 0.75rem;
    border-left: 4px solid var(--chrome-line);
    text-align: left;
  }
  .score-card.mine {
    border-left-color: var(--gold);
  }
  .score-card.winner {
    background: #2c3a1c;
  }
  .score-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .score-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .score-total {
    font-weight: bold;
    font-size: 1.3rem;
    color: var(--gold);
  }
  .score-detail {
    font-size: 0.8rem;
    color: var(--chrome-muted);
    margin-top: 0.25rem;
    margin-left: 22px;
  }
  .longest-banner {
    margin-top: 1rem;
    color: var(--gold);
    font-weight: bold;
    font-size: 1.1rem;
  }
  .ticket-results {
    margin: 0.25rem 0 0 22px;
    font-size: 0.8rem;
    color: var(--chrome-muted);
  }
  .ticket-results summary {
    cursor: pointer;
  }
  .ticket-results ul {
    list-style: none;
    margin: 0.2rem 0 0;
    padding: 0;
  }
</style>
