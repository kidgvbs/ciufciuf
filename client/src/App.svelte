<script lang="ts">
  // CiufCiuf! — Shell di routing client-side.
  // / → Table view (TV, mappa condivisa)
  // /controller → Controller view (telefono, mano privata)

  import TableView from './views/TableView.svelte';
  import ControllerView from './views/ControllerView.svelte';

  let currentPath = $state<string>(window.location.pathname);

  // Intercetta navigazione browser (back/forward)
  if (typeof window !== 'undefined') {
    window.addEventListener('popstate', () => {
      currentPath = window.location.pathname;
    });
  }
</script>

{#if currentPath === '/controller'}
  <ControllerView />
{:else}
  <TableView />
{/if}