/** The bit of `fetch` the extension uses, so tests can fake the network. */
export interface Reply {
  status: number;
  json(): Promise<unknown>;
}

export type Fetcher = (url: string, init: RequestInit) => Promise<Reply>;

export const realFetcher: Fetcher = (url, init) => fetch(url, init);

export class TimeoutError extends Error {
  override name = 'TimeoutError';
}

/** Runs `run` with an abort signal, rejecting with `TimeoutError` after `ms`. */
export function withTimeout<T>(ms: number, run: (signal: AbortSignal) => Promise<T>): Promise<T> {
  const ctl = new AbortController();
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      ctl.abort();
      reject(new TimeoutError(`no answer within ${ms} ms`));
    }, ms);
    run(ctl.signal).then(
      (v) => {
        clearTimeout(timer);
        resolve(v);
      },
      (e) => {
        clearTimeout(timer);
        reject(e);
      },
    );
  });
}

export const base = (port: number) => `http://127.0.0.1:${port}`;
