import { describe, it, expect } from 'vitest';
import {
  offerKey,
  isDialogOpen,
  nextDismissedKey,
  canConfirmTicketChoice,
  ticketChoiceHint,
  ticketChoiceProblem,
  type TicketChoiceRules,
} from '../ticket-offer';

const t = (id: string) => ({ id });

/** Simula lo stato del dialog nel componente: un PRIVATE_STATE alla volta. */
function makeDialog() {
  let dismissed = '';
  let key = '';
  return {
    privateState(offered: Array<{ id: string }>) {
      key = offerKey(offered);
      dismissed = nextDismissedKey(key, dismissed);
    },
    cancel() {
      dismissed = key;
    },
    reopen() {
      dismissed = '';
    },
    get open() {
      return isDialogOpen(key, dismissed);
    },
  };
}

describe('ticket offer dialog', () => {
  it('offerKey non dipende dall ordine', () => {
    expect(offerKey([t('b'), t('a'), t('c')])).toBe(offerKey([t('c'), t('a'), t('b')]));
    expect(offerKey([])).toBe('');
  });

  it('si apre quando compare un offerta e non senza offerta', () => {
    const d = makeDialog();
    d.privateState([]);
    expect(d.open).toBe(false);
    d.privateState([t('a'), t('b'), t('c')]);
    expect(d.open).toBe(true);
  });

  it('ticket_dialog_stays_closed_after_cancel_on_new_private_state', () => {
    const d = makeDialog();
    d.privateState([t('a'), t('b'), t('c')]);
    d.cancel();
    expect(d.open).toBe(false);

    // Nuovi PRIVATE_STATE con la stessa offerta (anche riordinata) non riaprono il dialog.
    d.privateState([t('a'), t('b'), t('c')]);
    d.privateState([t('c'), t('b'), t('a')]);
    expect(d.open).toBe(false);

    // Il pulsante "Devi scegliere i biglietti" lo riapre.
    d.reopen();
    expect(d.open).toBe(true);
  });

  it('un offerta con ID diversi riapre il dialog dopo Annulla', () => {
    const d = makeDialog();
    d.privateState([t('a'), t('b'), t('c')]);
    d.cancel();
    d.privateState([t('d'), t('e'), t('f')]);
    expect(d.open).toBe(true);
  });

  it('dopo la scelta la stessa offerta, ricomparsa più tardi, si riapre', () => {
    const d = makeDialog();
    d.privateState([t('a'), t('b'), t('c')]);
    d.cancel();
    d.privateState([]);
    d.privateState([t('a'), t('b'), t('c')]);
    expect(d.open).toBe(true);
  });
});

describe('conferma della scelta dei biglietti', () => {
  const rules = (keep_min: number, limits: TicketChoiceRules['limits'] = []): TicketChoiceRules => ({
    keep_min,
    limits,
  });
  const sel = (...ids: string[]) => new Set(ids);

  it('senza vincoli dal server non si conferma mai', () => {
    expect(canConfirmTicketChoice(sel('a', 'b', 'c'), null)).toBe(false);
  });

  it('serve almeno keep_min biglietti', () => {
    expect(canConfirmTicketChoice(sel('a'), rules(2))).toBe(false);
    expect(canConfirmTicketChoice(sel('a', 'b'), rules(2))).toBe(true);
    expect(canConfirmTicketChoice(sel('a'), rules(1))).toBe(true);
    expect(canConfirmTicketChoice(sel(), rules(1))).toBe(false);
  });

  it('un gruppo oltre il suo massimo blocca la conferma', () => {
    const r = rules(3, [{ ticket_ids: ['l1', 'l2'], min: 0, max: 1 }]);
    expect(canConfirmTicketChoice(sel('l1', 'a', 'b'), r)).toBe(true);
    expect(canConfirmTicketChoice(sel('l1', 'l2', 'a'), r)).toBe(false);
    expect(canConfirmTicketChoice(sel('l1', 'l2', 'a', 'b'), r)).toBe(false);
    expect(canConfirmTicketChoice(sel('a', 'b', 'c'), r)).toBe(true);
  });

  it('un gruppo sotto il suo minimo blocca la conferma (Germany: almeno 1 corto)', () => {
    // 2 lunghi + 3 corti, tieni almeno 2 di cui almeno 1 corto.
    const r = rules(2, [{ ticket_ids: ['r1', 'r2', 'r3'], min: 1, max: 3 }]);
    expect(canConfirmTicketChoice(sel('l1', 'l2'), r)).toBe(false);
    expect(canConfirmTicketChoice(sel('l1', 'r1'), r)).toBe(true);
    expect(canConfirmTicketChoice(sel('r1', 'r2'), r)).toBe(true);
    expect(canConfirmTicketChoice(sel('r1'), r)).toBe(false);
  });

  it('ticketChoiceHint: solo minimo, singolare e plurale', () => {
    const deck = () => undefined;
    expect(ticketChoiceHint(rules(2), deck)).toBe('Tieni almeno 2 biglietti');
    expect(ticketChoiceHint(rules(1), deck)).toBe('Tieni almeno 1 biglietto');
  });

  it('ticketChoiceHint: gruppo di biglietti lunghi', () => {
    const deck = (id: string) => (id.startsWith('l') ? 'long' : 'regular');
    expect(ticketChoiceHint(rules(3, [{ ticket_ids: ['l1', 'l2'], min: 0, max: 1 }]), deck)).toBe(
      'Tieni almeno 3 biglietti, al massimo 1 biglietto lungo',
    );
    expect(ticketChoiceHint(rules(3, [{ ticket_ids: ['l1', 'l2', 'l3'], min: 0, max: 2 }]), deck)).toBe(
      'Tieni almeno 3 biglietti, al massimo 2 biglietti lunghi',
    );
  });

  it('ticketChoiceHint: gruppo senza tetto con minimo (corti di Germany)', () => {
    const deck = (id: string) => (id.startsWith('l') ? 'long' : 'regular');
    expect(ticketChoiceHint(rules(2, [{ ticket_ids: ['r1', 'r2', 'r3'], min: 1, max: 3 }]), deck)).toBe(
      'Tieni almeno 2 biglietti, almeno 1 biglietto corto',
    );
  });

  it('ticketChoiceHint: gruppo non lungo o deck sconosciuto', () => {
    const deck = (id: string) => (id === 'l1' ? 'long' : 'regular');
    expect(ticketChoiceHint(rules(2, [{ ticket_ids: ['l1', 'r1'], min: 0, max: 1 }]), deck)).toBe(
      'Tieni almeno 2 biglietti, al massimo 1 biglietto di questo gruppo',
    );
    expect(ticketChoiceHint(rules(2, [{ ticket_ids: ['x', 'y'], min: 0, max: 1 }]), () => undefined)).toBe(
      'Tieni almeno 2 biglietti, al massimo 1 biglietto di questo gruppo',
    );
  });

  it('ticketChoiceProblem: messaggio per il gruppo violato, null se tutto ok', () => {
    const deck = (id: string) => (id.startsWith('l') ? 'long' : 'regular');
    const r = rules(2, [
      { ticket_ids: ['l1', 'l2'], min: 0, max: 1 },
      { ticket_ids: ['r1', 'r2', 'r3'], min: 1, max: 3 },
    ]);
    expect(ticketChoiceProblem(sel('l1', 'r1'), r, deck)).toBeNull();
    expect(ticketChoiceProblem(sel('l1'), r, deck)).toBe('Tieni almeno 1 biglietto corto');
    expect(ticketChoiceProblem(sel('l1', 'l2', 'r1'), r, deck)).toBe('Tieni al massimo 1 biglietto lungo');
    expect(ticketChoiceProblem(sel('r1'), null, deck)).toBeNull();
  });
});
