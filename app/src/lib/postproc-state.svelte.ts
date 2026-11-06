import type { Backend } from './backend';
import { errText } from './postproc';
import type { Run, Update } from './types';

export type PostprocState = ReturnType<typeof createPostprocState>;

/** The after-download runs, by package and by lone download. */
export function createPostprocState(
  backend: Backend,
  ui: { showToast: (text: string) => void; onChanged: () => void },
) {
  let runs = $state<Run[]>([]);

  // Newest load wins; a load requested while one is in flight runs once afterwards.
  let gen = 0;
  let inflight: Promise<void> | null = null;
  let again = false;

  async function load(): Promise<void> {
    if (inflight) {
      again = true;
      return inflight;
    }
    const my = ++gen;
    inflight = (async () => {
      try {
        const r = await backend.postprocRuns();
        if (my === gen) runs = r;
      } catch {
        /* keep what we have; the next update retries */
      } finally {
        inflight = null;
        if (again) {
          again = false;
          void load();
        }
      }
    })();
    return inflight;
  }

  function onUpdate(u: Update) {
    if (!u.postproc) return;
    void load();
    ui.onChanged();
  }

  async function act(f: () => Promise<unknown>): Promise<boolean> {
    try {
      await f();
      return true;
    } catch (e) {
      ui.showToast(errText(e));
      return false;
    } finally {
      void load();
    }
  }

  return {
    get runs() {
      return runs;
    },
    byPackage: (id: number) => runs.find((r) => r.packageId === id) ?? null,
    byDownload: (id: number) => runs.find((r) => r.downloadId === id) ?? null,
    load,
    onUpdate,
    process: (t: { packageId?: number; downloadId?: number }) => act(() => backend.postprocProcess(t)),
    retry: (runId: number) => act(() => backend.postprocRetry(runId)),
    /** A wrong password shows in the step's detail, not as a toast. */
    password: (runId: number, password: string, remember: boolean) =>
      act(() => backend.postprocPassword(runId, password, remember)),
    async setHash(id: number, hash: string | null): Promise<boolean> {
      const ok = await act(() => backend.setExpectedHash(id, hash));
      if (ok) ui.showToast('Checksum saved');
      return ok;
    },
  };
}
