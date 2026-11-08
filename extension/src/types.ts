/** What the user can change on the options page. */
export interface Options {
  /** Bytes: downloads at least this big are caught. */
  minSize: number;
  /** File types (lowercase, no dot) caught at any size. */
  types: string[];
}

export const DEFAULT_OPTIONS: Options = {
  minSize: 1048576,
  types: ['zip', 'rar', '7z', 'exe', 'msi', 'iso', 'mp4', 'mkv', 'mp3', 'flac', 'pdf'],
};

/** A browser download, as far as the catch rule cares. */
export interface DownloadFacts {
  url: string;
  finalUrl: string;
  /** The path the browser picked; may be empty this early. */
  filename: string;
  /** null when the server didn't say. */
  size: number | null;
  incognito: boolean;
}

/** An Alt+click on a link, recorded by the content script. */
export interface AltClick {
  url: string;
  at: number;
}

/** `POST /v1/capture`. */
export interface CaptureBody {
  url: string;
  filename?: string;
  size?: number;
  referrer?: string;
  user_agent?: string;
  /** `name=value; …`, as a Cookie header. */
  cookies?: string;
}
