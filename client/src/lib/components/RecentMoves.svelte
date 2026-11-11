<script lang="ts">
  // "Ultime mosse" nella plancia della TV: una riga compatta per mossa, la più recente in alto,
  // con pallino del giocatore. Nei tunnel le carte rivelate sono mini-tessere (quelle che alzano
  // il costo evidenziate, come in `TunnelCards`). Testi dal helper puro `recent-moves`.
  import type { RecentMove } from '../proto';
  import { CARD_COLOR_CSS, CARD_COLOR_NAME, PLAYER_COLOR_CSS } from '../colors';
  import { describeMove, latestMoves, type MoveContext } from '../recent-moves';

  let { moves, players, map }: {
    moves: readonly RecentMove[];
    players: MoveContext['players'];
    map: MoveContext['map'];
  } = $props();

  const lines = $derived(latestMoves(moves).map((m) => describeMove(m, { players, map })));
</script>

{#if lines.length > 0}
  <section class="recent-moves" aria-label="Ultime mosse">
    <h3>Ultime mosse</h3>
    <ol>
      {#each lines as l, i (i)}
        <li class:newest={i === 0} aria-label={l.text}>
          <span class="dot" style="background:{l.color ? PLAYER_COLOR_CSS[l.color] : '#666'}" aria-hidden="true"></span>
          <span class="txt" aria-hidden="true">
            <strong>{l.name}</strong>{l.rest}
            {#if l.cards}
              {#each l.cards as c, j (j)}
                <span class="chip" class:raises={c.raises} class:neutral={!c.raises}
                  style="--chip:{CARD_COLOR_CSS[c.color]}" title={CARD_COLOR_NAME[c.color]}>{c.color === 'LOCOMOTIVE' ? '★' : ''}</span>
              {/each}
              {l.suffix}
            {/if}
          </span>
        </li>
      {/each}
    </ol>
  </section>
{/if}

<style>
  .recent-moves {
    margin: 0.25rem 0 0.75rem;
    /* Cede altezza per prima: le righe più vecchie (in fondo) vengono tagliate. */
    overflow: hidden;
  }
  h3 {
    margin: 0 0 0.3rem;
    font-size: 0.85rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  ol {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: flex-start;
    gap: 0.4rem;
    padding: 0.2rem 0.1rem;
    border-bottom: 1px solid var(--chrome-line);
    font-size: 0.78rem;
    line-height: 1.25;
    color: var(--chrome-muted);
  }
  li:last-child {
    border-bottom: none;
  }
  li.newest {
    color: var(--chrome-text);
  }
  .dot {
    flex: none;
    width: 9px;
    height: 9px;
    margin-top: 0.22rem;
    border-radius: 50%;
    border: 1px solid rgba(255, 255, 255, 0.35);
  }
  .txt {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .txt strong {
    color: var(--chrome-text);
  }
  .chip {
    display: inline-block;
    width: 11px;
    height: 16px;
    margin: 0 1px;
    border-radius: 3px;
    border: 1px solid #1d150c;
    background: var(--chip);
    color: #fbf6e9;
    font-size: 0.6rem;
    line-height: 14px;
    text-align: center;
    vertical-align: middle;
  }
  .chip.raises {
    outline: 2px solid var(--gold);
    box-shadow: 0 0 5px rgba(232, 168, 62, 0.8);
  }
  .chip.neutral {
    opacity: 0.55;
  }
</style>
