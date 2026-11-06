// TS mirrors of app/src-tauri/src/dto.rs.

export type State = 'queued' | 'running' | 'paused' | 'completed' | 'failed' | 'link_expired' | 'offline';
export type PriorityName = 'high' | 'normal' | 'low';
/** [start, pos, end] of one byte range. `end` can exceed 2^53 when the size is unknown. */
export type Seg = [number, number, number];

export type IconName =
  | 'queue' | 'collector' | 'finished' | 'rules' | 'settings'
  | 'dl' | 'ok' | 'err' | 'pause' | 'play' | 'wait'
  | 'chev' | 'plus' | 'search' | 'folder' | 'link' | 'x';

export interface DownloadView {
  id: number;
  packageId: number | null;
  name: string;
  url: string;
  host: string;
  size: number | null;
  written: number;
  state: State;
  error: string | null;
  priority: PriorityName;
  position: number;
  connections: number;
  resumable: boolean | null;
  targetDir: string;
  targetPath: string | null;
  createdAt: string;
  segments: Seg[];
}

export interface PackageView {
  id: number;
  name: string;
  createdAt: string;
}

export interface SpeedLimit {
  enabled: boolean;
  bps: number;
}

export interface Snapshot {
  packages: PackageView[];
  downloads: DownloadView[];
  speedLimit: SpeedLimit;
}

export interface ProgressView {
  id: number;
  written: number;
  size: number | null;
  speedBps: number;
  segments: Seg[];
}

export interface Update {
  progress: ProgressView[];
  states: { id: number; state: State; error: string | null }[];
  steals: { id: number; bytes: number }[];
  resync: boolean;
  /** The Collector changed: reload its snapshot. */
  collector: boolean;
}

export type LinkStatus = 'checking' | 'online' | 'offline' | 'expired' | 'error';
export type DupKind = 'downloaded' | 'queued' | 'collector';
export type Source = 'clipboard' | 'paste' | 'drop';
export type ArchiveKind = 'rar' | '7z' | 'zip';

export interface CollectedLink {
  id: number;
  batchId: number;
  url: string;
  name: string;
  size: number | null;
  contentType: string | null;
  status: LinkStatus;
  message: string | null;
  source: string;
  duplicate: DupKind | null;
  /** null for a lone link. */
  packageKey: string | null;
}

export interface CollectorPackage {
  key: string;
  name: string;
  archive: ArchiveKind | null;
  source: string;
  createdAt: string;
}

export interface CollectorFilters {
  html: boolean;
  images: boolean;
  small: boolean;
  duplicates: boolean;
  /** Lowercase, no leading dot. */
  exts: string[];
}

export interface CollectorSnapshot {
  packages: CollectorPackage[];
  /** Display order: each package's links follow it. */
  links: CollectedLink[];
  filters: CollectorFilters;
  watchClipboard: boolean;
}

export interface IntakeResult {
  added: number;
  /** Links already waiting in the Collector. */
  skipped: number;
}

export interface StartResult {
  started: number;
}
