import type { Backend } from './backend';
import { errText } from './postproc';
import type { WhenDoneMode, WhenDoneStatus } from './types';

export type WhenDoneState = ReturnType<typeof createWhenDoneState>;

/** "When done": the mode and the countdown, kept in step with the tray through `haul:when-done`. */
export function createWhenDoneState(backend: Backend, ui: { showToast: (text: string) => void }) {
  let status = $state<WhenDoneStatus>({ mode: 'nothing', countdown: null });
  backend.onWhenDone((s) => (status = s));
  void backend.whenDone().then(
    (s) => (status = s),
    () => {},
  );

  async function run(f: () => Promise<WhenDoneStatus | void>) {
    try {
      const s = await f();
      if (s) status = s;
    } catch (e) {
      ui.showToast(errText(e));
    }
  }

  return {
    get status() {
      return status;
    },
    set: (mode: WhenDoneMode) => run(() => backend.setWhenDone(mode)),
    cancel: () => run(() => backend.whenDoneCancel()),
    now: () =>
      run(async () => {
        status = { mode: 'nothing', countdown: null };
        await backend.whenDoneNow();
      }),
  };
}
