<script lang="ts">
  // Mappa della partita: carica i dati dal server (store `gameMap`) e mostra `MapBoard`
  // quando sono pronti; nel frattempo mostra "Caricamento…" o l'errore con "Riprova".
  // Se il server non ha un layout per la mappa mostra "Layout della mappa non disponibile".
  // Un errore di rendering di `MapBoard` mostra "Mappa non disponibile" con "Riprova".

  import { gameMap } from '../game-map.svelte.ts';
  import { rulesetCatalog } from '../rulesets.svelte.ts';
  import MapBoard from './MapBoard.svelte';

  let props: {
    highlightRouteIds?: string[];
    selectedRouteId?: string | null;
    onRouteTap?: (routeId: string) => void;
    compact?: boolean;
    highlightTickets?: Array<{ from: string; to: string[] }>;
    selectableCityIds?: string[];
    selectedCityId?: string | null;
    onCityTap?: (cityId: string) => void;
    focusClaim?: { seq: number; routeId: string } | null;
  } = $props();

  const data = $derived(gameMap.state.data);
  const layout = $derived(gameMap.state.layout);
</script>

{#if data && !layout}
  <div class="map-status" role="status">
    <p>Layout della mappa non disponibile</p>
  </div>
{:else if data && layout}
  {#key data}
    <svelte:boundary>
      <MapBoard {...props} mapData={data} {layout} bigCities={rulesetCatalog.bigCities(gameMap.rulesetId)} />
      {#snippet failed(_error, reset)}
        <div class="map-status" role="alert">
          <p>Mappa non disponibile</p>
          <button
            class="btn"
            onclick={() => {
              reset();
              gameMap.retry();
            }}>Riprova</button
          >
        </div>
      {/snippet}
    </svelte:boundary>
  {/key}
{:else}
  <div class="map-status" role="status">
    {#if gameMap.state.status === 'error'}
      <p>Mappa non disponibile: {gameMap.state.error}</p>
      <button class="btn" onclick={() => gameMap.retry()}>Riprova</button>
    {:else}
      <p>Caricamento mappa…</p>
    {/if}
  </div>
{/if}

<style>
  .map-status {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.75rem;
    width: 100%;
    height: 100%;
    min-height: 8rem;
    color: var(--chrome-muted);
  }
  p {
    margin: 0;
  }
</style>
