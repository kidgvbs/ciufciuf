import { describe, it, expect, vi, afterEach } from 'vitest';
import { createOverlayDismiss } from '../overlay-dismiss';

const overlay = {};
const dialog = {};
const on = (target: object) => ({ target: target as EventTarget, currentTarget: overlay as EventTarget });

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('createOverlayDismiss', () => {
  it('chiude con pressione e rilascio sull’overlay', () => {
    const onDismiss = vi.fn();
    const d = createOverlayDismiss(onDismiss);
    d.onpointerdown(on(overlay));
    d.onpointerup(on(overlay));
    d.onclick(on(overlay));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it('overlay_press_outside_release_inside_does_not_close', () => {
    const onDismiss = vi.fn();
    const d = createOverlayDismiss(onDismiss);
    d.onpointerdown(on(overlay));
    d.onpointerup(on(dialog));
    // Il browser manda il click all'antenato comune: l'overlay.
    d.onclick(on(overlay));
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it('overlay_click_started_inside_does_not_close', () => {
    const onDismiss = vi.fn();
    const d = createOverlayDismiss(onDismiss);
    d.onpointerdown(on(dialog));
    d.onpointerup(on(overlay));
    d.onclick(on(overlay));
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it('un clic sul dialog non chiude', () => {
    const onDismiss = vi.fn();
    const d = createOverlayDismiss(onDismiss);
    d.onpointerdown(on(dialog));
    d.onpointerup(on(dialog));
    d.onclick(on(dialog));
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it('dopo un clic la pressione registrata si azzera', () => {
    const onDismiss = vi.fn();
    const d = createOverlayDismiss(onDismiss);
    d.onpointerdown(on(overlay));
    d.onpointerup(on(overlay));
    d.onclick(on(overlay));
    d.onclick(on(overlay));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it('touch_release_over_dialog_with_captured_target_does_not_close', () => {
    // Col tocco il target del pointerup è l'overlay (cattura implicita) ma il dito è sul dialog.
    vi.stubGlobal('document', { elementFromPoint: () => dialog });
    const onDismiss = vi.fn();
    const d = createOverlayDismiss(onDismiss);
    d.onpointerdown(on(overlay));
    d.onpointerup({ ...on(overlay), clientX: 10, clientY: 10 });
    d.onclick(on(overlay));
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it('touch_release_over_overlay_closes', () => {
    vi.stubGlobal('document', { elementFromPoint: () => overlay });
    const onDismiss = vi.fn();
    const d = createOverlayDismiss(onDismiss);
    d.onpointerdown(on(overlay));
    d.onpointerup({ ...on(overlay), clientX: 10, clientY: 10 });
    d.onclick(on(overlay));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });
});
