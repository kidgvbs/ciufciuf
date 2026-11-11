import { describe, it, expect, vi } from 'vitest';
import { adoptGame } from '../adopt-game';

function setup() {
  let alive = true;
  let finishRefresh!: () => void;
  const lobbyRefreshed = new Promise<void>((resolve) => { finishRefresh = resolve; });
  const steps = {
    isAlive: () => alive,
    adopt: vi.fn(),
    refreshLobby: vi.fn(() => lobbyRefreshed),
    connect: vi.fn(),
  };
  return { steps, unmount: () => { alive = false; }, finishRefresh };
}

describe('adoptGame', () => {
  it('registra la partita, aggiorna la lobby e apre la connessione', async () => {
    const { steps, finishRefresh } = setup();
    const done = adoptGame(steps);
    expect(steps.adopt).toHaveBeenCalledTimes(1);
    expect(steps.connect).not.toHaveBeenCalled();
    finishRefresh();
    await done;
    expect(steps.connect).toHaveBeenCalledTimes(1);
  });

  it('late_resume_after_unmount_opens_no_socket', async () => {
    const { steps, unmount, finishRefresh } = setup();
    const done = adoptGame(steps);
    unmount();
    finishRefresh();
    await done;
    expect(steps.connect).not.toHaveBeenCalled();
  });

  it('una risposta che arriva a vista già smontata non fa nulla', async () => {
    const { steps, unmount, finishRefresh } = setup();
    unmount();
    finishRefresh();
    await adoptGame(steps);
    expect(steps.adopt).not.toHaveBeenCalled();
    expect(steps.refreshLobby).not.toHaveBeenCalled();
    expect(steps.connect).not.toHaveBeenCalled();
  });
});
