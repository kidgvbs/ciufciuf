import { describe, it, expect } from 'vitest';
import { offerKey, isDialogOpen, nextDismissedKey } from '../ticket-offer';

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
