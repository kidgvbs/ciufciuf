import { describe, it, expect } from 'vitest';
import {
  offeredTicketPins,
  ticketDestinations,
  ticketPin,
  ticketLayers,
  ticketPinColor,
  TICKET_PIN_COLORS,
} from '../map-utils';

const tickets = [
  { id: 'a', from: 'c1', to: 'c2', points: 5 },
  { id: 'b', from: 'c3', to: 'c4', points: 6 },
  { id: 'c', from: 'c5', to: 'c6', points: 7 },
  {
    id: 'm',
    from: 'fr',
    destinations: [
      { to: 'de', points: 5 },
      { to: 'it', points: 13 },
      { to: 'at', points: 14 },
    ],
  },
];

const all = new Set(['a', 'b', 'c', 'm', 'zz']);
const empty = { from: '', to: [] };

describe('ticketDestinations', () => {
  it('il biglietto classico ha una sola destinazione', () => {
    expect(ticketDestinations(tickets[0]!)).toEqual([{ to: 'c2', points: 5 }]);
  });

  it('il biglietto multi-destinazione le elenca tutte nell\'ordine della mappa', () => {
    expect(ticketDestinations(tickets[3]!).map((d) => d.to)).toEqual(['de', 'it', 'at']);
  });
});

describe('ticketPin', () => {
  it('origine e tutte le destinazioni', () => {
    expect(ticketPin(tickets[0]!)).toEqual({ from: 'c1', to: ['c2'] });
    expect(ticketPin(tickets[3]!)).toEqual({ from: 'fr', to: ['de', 'it', 'at'] });
  });
});

describe('offeredTicketPins', () => {
  it('segue l\'ordine dei biglietti offerti, non quello della mappa', () => {
    expect(offeredTicketPins([{ id: 'c' }, { id: 'a' }], tickets, all)).toEqual([
      { from: 'c5', to: ['c6'] },
      { from: 'c1', to: ['c2'] },
    ]);
  });

  it('solo i selezionati sono tracciati; gli altri restano pin vuoti senza spostare i colori', () => {
    const offered = [{ id: 'a' }, { id: 'b' }, { id: 'c' }];
    expect(offeredTicketPins(offered, tickets, new Set())).toEqual([empty, empty, empty]);
    expect(offeredTicketPins(offered, tickets, new Set(['c']))).toEqual([
      empty,
      empty,
      { from: 'c5', to: ['c6'] },
    ]);
  });

  it('un biglietto multi-destinazione evidenzia origine e tutte le destinazioni', () => {
    expect(offeredTicketPins([{ id: 'm' }], tickets, all)).toEqual([
      { from: 'fr', to: ['de', 'it', 'at'] },
    ]);
  });

  it('un descrittore mancante lascia un pin vuoto e non sposta i colori dei successivi', () => {
    const pins = offeredTicketPins([{ id: 'a' }, { id: 'zz' }, { id: 'b' }], tickets, all);
    expect(pins).toEqual([
      { from: 'c1', to: ['c2'] },
      empty,
      { from: 'c3', to: ['c4'] },
    ]);
  });

  it('senza dati di mappa tutti i pin sono vuoti', () => {
    expect(offeredTicketPins([{ id: 'a' }], [], all)).toEqual([empty]);
  });
});

describe('ticketPinColor', () => {
  it('usa la palette per indice e la ripete oltre la fine', () => {
    expect(ticketPinColor(0)).toBe(TICKET_PIN_COLORS[0]);
    expect(ticketPinColor(TICKET_PIN_COLORS.length)).toBe(TICKET_PIN_COLORS[0]);
    expect(ticketPinColor(1)).toBe(TICKET_PIN_COLORS[1]);
  });
});

describe('ticketLayers', () => {
  const positions = new Map([
    ['fr', { x: 0, y: 0 }],
    ['de', { x: 100, y: 0 }],
    ['it', { x: 0, y: 100 }],
    ['at', { x: 100, y: 100 }],
    ['dup', { x: 0, y: 0 }],
  ]);

  it('biglietto classico: due pin e una linea', () => {
    const [layer, ...rest] = ticketLayers([{ from: 'fr', to: ['de'] }], positions);
    expect(rest).toEqual([]);
    expect(layer!.pins).toEqual([{ x: 0, y: 0 }, { x: 100, y: 0 }]);
    expect(layer!.paths).toHaveLength(1);
    expect(layer!.paths[0]).toMatch(/^M 0 0 Q /);
    expect(layer!.color).toBe(ticketPinColor(0));
  });

  it('multi-destinazione: un pin sull\'origine e uno su ogni destinazione, una linea ciascuna, stesso colore', () => {
    const layers = ticketLayers(
      [
        { from: '', to: [] },
        { from: 'fr', to: ['de', 'it', 'at'] },
      ],
      positions,
    );
    expect(layers).toHaveLength(1);
    expect(layers[0]!.pins).toEqual([
      { x: 0, y: 0 },
      { x: 100, y: 0 },
      { x: 0, y: 100 },
      { x: 100, y: 100 },
    ]);
    expect(layers[0]!.paths).toHaveLength(3);
    expect(layers[0]!.color).toBe(ticketPinColor(1));
  });

  it('salta destinazioni mancanti o sull\'origine, e i biglietti senza nulla da disegnare', () => {
    const layers = ticketLayers(
      [
        { from: 'fr', to: ['nowhere', 'dup', 'it'] },
        { from: 'fr', to: ['dup'] },
        { from: 'nowhere', to: ['de'] },
      ],
      positions,
    );
    expect(layers).toHaveLength(1);
    expect(layers[0]!.pins).toEqual([{ x: 0, y: 0 }, { x: 0, y: 100 }]);
    expect(layers[0]!.paths).toHaveLength(1);
  });
});
