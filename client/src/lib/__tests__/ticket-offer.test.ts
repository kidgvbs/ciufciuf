import { describe, it, expect } from 'vitest';
import {
  offerKey,
  isDialogOpen,
  nextDismissedKey,
  canConfirmTicketChoice,
  ticketChoiceHint,
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
    const r = rules(3, [{ ticket_ids: ['l1', 'l2'], max: 1 }]);
    expect(canConfirmTicketChoice(sel('l1', 'a', 'b'), r)).toBe(true);
    expect(canConfirmTicketChoice(sel('l1', 'l2', 'a'), r)).toBe(false);
    expect(canConfirmTicketChoice(sel('l1', 'l2', 'a', 'b'), r)).toBe(false);
    expect(canConfirmTicketChoice(sel('a', 'b', 'c'), r)).toBe(true);
  });

  it('ticketChoiceHint: solo minimo, singolare e plurale', () => {
    const deck = () => undefined;
    expect(ticketChoiceHint(rules(2), deck)).toBe('Tieni almeno 2 biglietti');
    expect(ticketChoiceHint(rules(1), deck)).toBe('Tieni almeno 1 biglietto');
  });

  it('ticketChoiceHint: gruppo di biglietti lunghi', () => {
    const deck = (id: string) => (id.startsWith('l') ? 'long' : 'regular');
    expect(ticketChoiceHint(rules(3, [{ ticket_ids: ['l1', 'l2'], max: 1 }]), deck)).toBe(
      'Tieni almeno 3 biglietti, al massimo 1 biglietto lungo',
    );
    expect(ticketChoiceHint(rules(3, [{ ticket_ids: ['l1', 'l2', 'l3'], max: 2 }]), deck)).toBe(
      'Tieni almeno 3 biglietti, al massimo 2 biglietti lunghi',
    );
  });

  it('ticketChoiceHint: gruppo non lungo o deck sconosciuto', () => {
    const deck = (id: string) => (id === 'l1' ? 'long' : 'regular');
    expect(ticketChoiceHint(rules(2, [{ ticket_ids: ['l1', 'r1'], max: 1 }]), deck)).toBe(
      'Tieni almeno 2 biglietti, al massimo 1 biglietto di questo gruppo',
    );
    expect(ticketChoiceHint(rules(2, [{ ticket_ids: ['x'], max: 2 }]), () => undefined)).toBe(
      'Tieni almeno 2 biglietti, al massimo 2 biglietti di questo gruppo',
    );
  });
});
