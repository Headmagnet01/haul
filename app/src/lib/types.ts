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
}

export interface AddResult {
  added: number;
  packageId: number | null;
}
