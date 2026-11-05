import type { AddResult, PriorityName, Snapshot, SpeedLimit, Update } from './types';
import { createMockBackend } from './backend-mock';
import { tauriBackend } from './backend-tauri';

export interface Backend {
  snapshot(): Promise<Snapshot>;
  addLinks(text: string): Promise<AddResult>;
  pause(ids: number[]): Promise<void>;
  resume(ids: number[]): Promise<void>;
  remove(ids: number[]): Promise<void>;
  pauseAll(): Promise<void>;
  setPriority(id: number, p: PriorityName): Promise<void>;
  reorder(ids: number[]): Promise<void>;
  setSpeedLimit(l: SpeedLimit): Promise<void>;
  openFolder(id: number): Promise<void>;
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
    instance = mock;
  } else {
    instance = tauriBackend;
  }
  return instance;
}
