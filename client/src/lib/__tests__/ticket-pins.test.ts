import { describe, it, expect } from 'vitest';
import { offeredTicketPins, ticketPinColor, TICKET_PIN_COLORS } from '../map-utils';

const tickets = [
  { id: 'a', from: 'c1', to: 'c2' },
  { id: 'b', from: 'c3', to: 'c4' },
  { id: 'c', from: 'c5', to: 'c6' },
];

const all = new Set(['a', 'b', 'c', 'zz']);

describe('offeredTicketPins', () => {
  it('segue l\'ordine dei biglietti offerti, non quello della mappa', () => {
    expect(offeredTicketPins([{ id: 'c' }, { id: 'a' }], tickets, all)).toEqual([
      { from: 'c5', to: 'c6' },
      { from: 'c1', to: 'c2' },
    ]);
  });

  it('solo i selezionati sono tracciati; gli altri restano coppie vuote senza spostare i colori', () => {
    const offered = [{ id: 'a' }, { id: 'b' }, { id: 'c' }];
    expect(offeredTicketPins(offered, tickets, new Set())).toEqual([
      { from: '', to: '' },
      { from: '', to: '' },
      { from: '', to: '' },
    ]);
    expect(offeredTicketPins(offered, tickets, new Set(['c']))).toEqual([
      { from: '', to: '' },
      { from: '', to: '' },
      { from: 'c5', to: 'c6' },
    ]);
  });

  it('un descrittore mancante lascia una coppia vuota e non sposta i colori dei successivi', () => {
    const pins = offeredTicketPins([{ id: 'a' }, { id: 'zz' }, { id: 'b' }], tickets, all);
    expect(pins).toEqual([
      { from: 'c1', to: 'c2' },
      { from: '', to: '' },
      { from: 'c3', to: 'c4' },
    ]);
  });

  it('senza dati di mappa tutte le coppie sono vuote', () => {
    expect(offeredTicketPins([{ id: 'a' }], [], all)).toEqual([{ from: '', to: '' }]);
  });
});

describe('ticketPinColor', () => {
  it('usa la palette per indice e la ripete oltre la fine', () => {
    expect(ticketPinColor(0)).toBe(TICKET_PIN_COLORS[0]);
    expect(ticketPinColor(TICKET_PIN_COLORS.length)).toBe(TICKET_PIN_COLORS[0]);
    expect(ticketPinColor(1)).toBe(TICKET_PIN_COLORS[1]);
  });
});
