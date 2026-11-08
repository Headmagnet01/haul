import type {
  Category, CollectorFilters, CollectorSnapshot, Destination, IntakeResult, PostProcSettings, PriorityName, Rule,
  PairedClient, RulesConfig, Run, Settings, SettingsPatch, Snapshot, Source, StartResult, Update,
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
    const mock = createMockBackend({ tickMs: 250, demo });
    (window as unknown as { __haulMockCalls: string[] }).__haulMockCalls = mock.calls;
    // e2e tests play the browser extension.
    (window as unknown as { __haulMock: { pairRequest(b: string): number; pairTimeout(): void } }).__haulMock = {
      pairRequest: (b) => mock.pairRequest(b),
      pairTimeout: () => mock.pairTimeout(),
    };
    instance = mock;
  } else {
    instance = tauriBackend;
  }
  return instance;
}
