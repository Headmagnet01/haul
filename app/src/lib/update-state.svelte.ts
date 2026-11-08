import type { Backend } from './backend';
import { errText } from './postproc';
import type { UpdateStatus } from './types';

export type UpdateState = ReturnType<typeof createUpdateState>;

/** The update check: the status bar's notice and Settings › App's Check now. */
/** `notify` shows a toast (the app's), for a release page that wouldn't open. */
export function createUpdateState(backend: Backend, notify: (text: string) => void = () => {}) {
  let status = $state<UpdateStatus | null>(null);
  let checking = $state(false);

  backend.onUpdateAvailable(({ version, url }) => {
    const base = status ?? { current: '', latest: null, url: null, checkedAt: null, error: null };
    status = { ...base, latest: version, url, error: null };
  });

  async function load() {
    try {
      status = await backend.updateStatus();
    } catch {
      /* no notice then */
    }
  }

  async function checkNow() {
    checking = true;
    try {
      status = await backend.checkUpdatesNow();
    } catch (e) {
      const base = status ?? { current: '', latest: null, url: null, checkedAt: null, error: null };
      status = { ...base, error: errText(e) };
    } finally {
      checking = false;
    }
  }

  return {
    get status() {
      return status;
    },
    get checking() {
      return checking;
    },
    load,
    checkNow,
    /** Opens the newer release's page. */
    async open() {
      if (!status?.url) return;
      try {
        await backend.openRelease(status.url);
      } catch (e) {
        notify(`Couldn't open the release page: ${errText(e)}`);
      }
    },
    /** The line under Check now; empty before the first check. */
    message(): string {
      if (!status) return '';
      if (status.error) return `Couldn't check: ${status.error}.`;
      if (status.latest) return `Version ${status.latest} is available.`;
      if (status.checkedAt) return `You have the latest version (${status.current}).`;
      return '';
    },
  };
}
