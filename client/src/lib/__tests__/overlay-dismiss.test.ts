import { describe, it, expect, vi } from 'vitest';
import { createOverlayDismiss } from '../overlay-dismiss';

const overlay = {};
const dialog = {};
const on = (target: object) => ({ target: target as EventTarget, currentTarget: overlay as EventTarget });

describe('createOverlayDismiss', () => {
  it('chiude con pressione e rilascio sull’overlay', () => {
    const onDismiss = vi.fn();
    const d = createOverlayDismiss(onDismiss);
    d.onpointerdown(on(overlay));
    d.onclick(on(overlay));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it('overlay_click_started_inside_does_not_close', () => {
    const onDismiss = vi.fn();
    const d = createOverlayDismiss(onDismiss);
    d.onpointerdown(on(dialog));
    d.onclick(on(overlay));
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it('un clic sul dialog non chiude', () => {
    const onDismiss = vi.fn();
    const d = createOverlayDismiss(onDismiss);
    d.onpointerdown(on(dialog));
    d.onclick(on(dialog));
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it('dopo un clic la pressione registrata si azzera', () => {
    const onDismiss = vi.fn();
    const d = createOverlayDismiss(onDismiss);
    d.onpointerdown(on(overlay));
    d.onclick(on(overlay));
    d.onclick(on(overlay));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });
});
