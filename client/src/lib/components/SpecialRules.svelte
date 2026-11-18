<script lang="ts">
  // Elenco puntato delle regole speciali del ruleset (testi dal server, `RulesetInfo.specialRules`).
  // Condiviso da TV (pannello grande, leggibile a distanza) e controller (modale).

  let { title, rules, large = false }: {
    /** Titolo del ruleset («Germany · Deutschland»); `null` se il catalogo non lo conosce. */
    title: string | null;
    rules: readonly string[];
    /** Font grande per la TV. */
    large?: boolean;
  } = $props();
</script>

<section class="special-rules" class:large aria-labelledby="special-rules-title">
  <h2 id="special-rules-title">Regole speciali{title ? ` — ${title}` : ''}</h2>
  <ul>
    {#each rules as rule}
      <li>{rule}</li>
    {/each}
  </ul>
</section>

<style>
  .special-rules {
    color: var(--chrome-text);
    text-align: left;
  }
  h2 {
    color: var(--gold);
    margin: 0 0 0.5rem;
    font-size: 1.1rem;
  }
  ul {
    margin: 0;
    padding-left: 1.2rem;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    font-size: 0.9rem;
    line-height: 1.35;
  }
  .large h2 {
    font-size: 2rem;
    margin-bottom: 0.9rem;
  }
  .large ul {
    gap: 0.6rem;
    padding-left: 1.6rem;
    font-size: 1.35rem;
    line-height: 1.35;
  }
  /* Telefono in orizzontale: testo più compatto per far stare più voci */
  @media (max-height: 500px) {
    h2 {
      font-size: 1rem;
      margin-bottom: 0.35rem;
    }
    ul {
      gap: 0.2rem;
      font-size: 0.8rem;
      line-height: 1.25;
    }
  }
</style>
