// TS mirrors of app/src-tauri/src/dto.rs.

export type State = 'queued' | 'running' | 'paused' | 'completed' | 'failed' | 'link_expired' | 'offline';
export type PriorityName = 'high' | 'normal' | 'low';
/** [start, pos, end] of one byte range. `end` can exceed 2^53 when the size is unknown. */
export type Seg = [number, number, number];

export type IconName =
  | 'queue' | 'collector' | 'finished' | 'rules' | 'settings'
  | 'dl' | 'ok' | 'err' | 'pause' | 'play' | 'wait'
  | 'chev' | 'plus' | 'search' | 'folder' | 'link' | 'x' | 'ext';

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
  /** A post-processing run changed: reload the runs and the snapshot. */
  postproc: boolean;
}

export type LinkStatus = 'checking' | 'online' | 'offline' | 'expired' | 'error';
export type DupKind = 'downloaded' | 'queued' | 'collector';
export type Source = 'clipboard' | 'paste' | 'drop' | 'browser';
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

// Post-processing and rules (plan 4).

export type StepKind = 'verify' | 'extract' | 'sort' | 'scan' | 'notify';
export type StepState = 'pending' | 'running' | 'done' | 'skipped' | 'failed' | 'needs_password';
export type RunState = 'held' | 'queued' | 'running' | 'waiting' | 'needs_password' | 'failed' | 'done';

export interface Step {
  kind: StepKind;
  state: StepState;
  detail: string | null;
}

/** One package's (or lone file's) after-download run. */
export interface Run {
  id: number;
  packageId: number | null;
  downloadId: number | null;
  state: RunState;
  steps: Step[];
  updatedAt: string;
}

export interface PostProcSettings {
  auto: boolean;
  sortCategories: boolean;
  recycleArchives: boolean;
  virusScan: boolean;
}

export interface Category {
  name: string;
  /** Lowercase, no dot. */
  exts: string[];
  /** Absolute, or relative to the download folder. */
  folder: string;
}

export interface RuleMatch {
  host: string | null;
  /** A glob, or a regex written /…/. */
  name: string | null;
  exts: string[];
  minSize: number | null;
  maxSize: number | null;
}

export interface RuleAction {
  folder: string | null;
  rename: string | null;
}

export interface Rule {
  id: number;
  name: string;
  enabled: boolean;
  match: RuleMatch;
  action: RuleAction;
}

export interface RulesConfig {
  settings: PostProcSettings;
  categories: Category[];
  rules: Rule[];
  /** Saved archive passwords, by id only: the passwords never reach the UI. */
  passwords: number[];
}

export interface Destination {
  /** null: the file stays where it is. */
  folder: string | null;
  name: string;
  /** `Rule “<name>”`, a category name, or '' when nothing matched. */
  by: string;
}

/** A browser extension paired with Haul. */
export interface PairedClient {
  id: number;
  browser: string;
  createdAt: string;
  lastSeen: string | null;
}
