import type {
  Category, CollectorFilters, CollectorSnapshot, Destination, IntakeResult, PostProcSettings, PriorityName, Rule,
  PairedClient, RulesConfig, Run, Settings, SettingsPatch, Snapshot, Source, StartResult, Update, WhenDoneMode,
  UpdateStatus, WhenDoneStatus,
} from './types';
import { createMockBackend } from './backend-mock';
import { tauriBackend } from './backend-tauri';

export interface Backend {
  settings(): Promise<Settings>;
  /** Saves and applies the fields in `p`; returns the whole record. Rejects with `"<field>: <reason>"`, saving nothing. */
  setSettings(p: SettingsPatch): Promise<Settings>;
  /** The Windows folder picker; null when cancelled. */
  pickFolder(): Promise<string | null>;
  /** Opens a folder in Explorer. */
  openPath(path: string): Promise<void>;
  snapshot(): Promise<Snapshot>;
  /** Sends the links in `text` to the Collector. */
  addLinks(text: string, source: Source): Promise<IntakeResult>;
  collectorSnapshot(): Promise<CollectorSnapshot>;
  collectorStart(ids: number[]): Promise<StartResult>;
  collectorRemove(ids: number[]): Promise<void>;
  collectorRecheck(ids: number[]): Promise<void>;
  setCollectorFilters(f: CollectorFilters): Promise<void>;
  pause(ids: number[]): Promise<void>;
  resume(ids: number[]): Promise<void>;
  remove(ids: number[]): Promise<void>;
  pauseAll(): Promise<void>;
  setPriority(id: number, p: PriorityName): Promise<void>;
  reorder(ids: number[]): Promise<void>;
  openFolder(id: number): Promise<void>;
  /** Opens a finished file with its default app. */
  openFile(id: number): Promise<void>;
  /** Explorer with the saved file selected (the same as `openFolder` for a finished file). */
  reveal(id: number): Promise<void>;
  /** The ids whose saved file is still on disk. */
  filesExist(ids: number[]): Promise<number[]>;
  /** Recycles each saved file, then removes its row. Rejects with `"<name> is in use"` or the OS message at the first failure. */
  deleteFiles(ids: number[]): Promise<void>;
  /** Adds the same URL as a new download; returns its id. */
  downloadAgain(id: number): Promise<number>;
  postprocRuns(): Promise<Run[]>;
  /** Runs a finished package (or lone file) now. Returns the run id. */
  postprocProcess(t: { packageId?: number; downloadId?: number }): Promise<number>;
  postprocRetry(runId: number): Promise<void>;
  postprocPassword(runId: number, password: string, remember: boolean): Promise<void>;
  /** Rejects with the message when the text isn't a checksum. */
  setExpectedHash(id: number, hash: string | null): Promise<void>;
  rulesConfig(): Promise<RulesConfig>;
  setPostprocSettings(s: PostProcSettings): Promise<void>;
  setCategories(c: Category[]): Promise<void>;
  /** Rejects with the first invalid rule's message; nothing is saved then. */
  setRules(r: Rule[]): Promise<void>;
  /** Where a file would go under a draft of the rules. */
  tryRules(
    facts: { name: string; host: string; size: number | null },
    draft: { rules: Rule[]; categories: Category[]; sort: boolean },
  ): Promise<Destination>;
  addPassword(password: string): Promise<number>;
  removePassword(id: number): Promise<void>;
  orderPasswords(ids: number[]): Promise<void>;
  /** Answers pairing request `n`. An answer for a request that already ended does nothing. */
  pairAnswer(n: number, allow: boolean): Promise<void>;
  pairedClients(): Promise<PairedClient[]>;
  removePairedClient(id: number): Promise<void>;
  /** A browser extension asks to pair (request `n`). Returns an unsubscribe function. */
  onPairRequest(cb: (n: number, browser: string) => void): () => void;
  /** Request `n` ended (answered, timed out, or the browser went away). Returns an unsubscribe function. */
  onPairClosed(cb: (n: number) => void): () => void;
  /** "When done" is never saved: it starts as Nothing. */
  whenDone(): Promise<WhenDoneStatus>;
  setWhenDone(mode: WhenDoneMode): Promise<WhenDoneStatus>;
  /** Cancel on the countdown: back to Nothing. */
  whenDoneCancel(): Promise<WhenDoneStatus>;
  /** Do it now on the countdown. */
  whenDoneNow(): Promise<void>;
  /** The mode or countdown changed (from the tray, the countdown, or a command). Returns an unsubscribe function. */
  onWhenDone(cb: (s: WhenDoneStatus) => void): () => void;
  /** Opens the browser extension's folder in Explorer (for Load unpacked). */
  openExtensionFolder(): Promise<void>;
  /** Haul asks the window to show a view (a click on a notification). Returns an unsubscribe function. */
  onShowView(cb: (view: string) => void): () => void;
  /** The last update check's result, kept in memory by Haul. */
  updateStatus(): Promise<UpdateStatus>;
  /** Asks GitHub now, whatever the switch says. */
  checkUpdatesNow(): Promise<UpdateStatus>;
  /** Opens a Haul release page in the browser; any other address is refused. */
  openRelease(url: string): Promise<void>;
  /** The daily check found a newer release. Returns an unsubscribe function. */
  onUpdateAvailable(cb: (u: { version: string; url: string }) => void): () => void;
  /** Returns an unsubscribe function. */
  onUpdate(cb: (u: Update) => void): () => void;
}

export { createMockBackend };

let instance: Backend | null = null;

/** The mock (`npm run dev:mock`, `?demo=1` seeds the mockup's rows) or the real app. */
export function getBackend(): Backend {
  if (instance) return instance;
  if (import.meta.env.VITE_BACKEND === 'mock') {
    const demo = new URLSearchParams(location.search).get('demo') === '1';
    const update = new URLSearchParams(location.search).get('update') ?? undefined;
    const mock = createMockBackend({ tickMs: 250, demo, update });
    (window as unknown as { __haulMockCalls: string[] }).__haulMockCalls = mock.calls;
    // e2e tests play the browser extension.
    (window as unknown as { __haulMock: object }).__haulMock = {
      pairRequest: (b: string) => mock.pairRequest(b),
      pairTimeout: () => mock.pairTimeout(),
      queueDrained: () => mock.queueDrained(),
      showView: (v: string) => mock.showView(v),
      updateAvailable: (v: string) => mock.updateAvailable(v),
    };
    instance = mock;
  } else {
    instance = tauriBackend;
  }
  return instance;
}
