// The mock's post-processing: one step every other tick, so the drawer can show each one running.
// A name with "secret" needs the password "haul"; one with "damaged" fails to extract.

import type {
  Category, Destination, DownloadView, PackageView, PostProcSettings, Rule, RulesConfig, Run, RunState, Step,
  StepKind,
} from './types';

const DIR = 'C:\\Users\\you\\Downloads';
const KINDS: StepKind[] = ['verify', 'extract', 'sort', 'scan', 'notify'];
const ARCHIVE = /\.(zip|rar|7z|tar|gz|tgz)$|\.(7z|zip)\.\d{3}$|\.part\d+\.rar$|\.r\d\d$/i;

const cat = (name: string, exts: string): Category => ({ name, exts: exts ? exts.split(' ') : [], folder: name });
export const MOCK_CATEGORIES: Category[] = [
  cat('Video', 'mp4 mkv avi mov wmv flv webm m4v mpg mpeg ts 3gp'),
  cat('Music', 'mp3 flac wav aac ogg opus m4a wma aiff'),
  cat('Documents', 'pdf doc docx xls xlsx ppt pptx odt ods odp txt rtf csv epub md'),
  cat('Programs', 'exe msi msix appx bat cmd ps1 iso img apk'),
  cat('Compressed', 'zip rar 7z tar gz tgz bz2 xz zst cab'),
  cat('Other', ''),
];

function categoryOf(name: string, cats: Category[]): Category {
  const ext = name.includes('.') ? name.split('.').at(-1)!.toLowerCase() : '';
  return (
    cats.find((c) => ext !== '' && c.exts.includes(ext)) ??
    (ARCHIVE.test(name) ? cats.find((c) => c.name === 'Compressed') : undefined) ??
    cats.find((c) => c.exts.length === 0) ??
    cats[cats.length - 1]
  );
}

function nameRegex(r: Rule): RegExp | null {
  const p = r.match.name?.trim();
  if (!p) return null;
  try {
    if (p.length >= 2 && p.startsWith('/') && p.endsWith('/')) return new RegExp(p.slice(1, -1), 'i');
    const esc = p.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*').replace(/\?/g, '.');
    return new RegExp(`^${esc}$`, 'i');
  } catch (e) {
    throw `Bad pattern in “${r.name.trim()}”: ${(e as Error).message}`;
  }
}

function validate(rules: Rule[]) {
  for (const r of rules) {
    if (!r.name.trim()) throw 'Rule needs a name';
    nameRegex(r);
    if (r.match.minSize !== null && r.match.maxSize !== null && r.match.minSize > r.match.maxSize)
      throw `Min size is bigger than max size in “${r.name.trim()}”`;
    if (!r.action.folder?.trim() && !r.action.rename?.trim()) throw `Rule “${r.name.trim()}” has no folder or rename`;
  }
}

const resolve = (folder: string) => (/^[a-z]:\\/i.test(folder) ? folder : `${DIR}\\${folder}`);

function decide(name: string, rules: Rule[], cats: Category[], sort: boolean): Destination {
  for (const r of rules) {
    if (!r.enabled) continue;
    const re = nameRegex(r);
    const ext = name.includes('.') ? name.split('.').at(-1)!.toLowerCase() : '';
    if (re && !re.test(name)) continue;
    if (r.match.exts.length && !r.match.exts.includes(ext)) continue;
    return { folder: r.action.folder ? resolve(r.action.folder) : null, name, by: `Rule “${r.name}”` };
  }
  if (sort) {
    const c = categoryOf(name, cats);
    return { folder: resolve(c.folder), name, by: c.name };
  }
  return { folder: null, name, by: '' };
}

interface MockRun extends Run {
  given: string | null;
  remember: boolean;
}

export function createMockPostproc(o: {
  changed: () => void;
  downloads: () => DownloadView[];
  packages: () => PackageView[];
}) {
  let nextRun = 1;
  let nextPw = 1;
  const runs: MockRun[] = [];
  const hashes = new Map<number, string>();
  let settings: PostProcSettings = { auto: true, sortCategories: true, recycleArchives: false, virusScan: false };
  let categories = structuredClone(MOCK_CATEGORIES);
  let rules: Rule[] = [];
  let passwords: { id: number; pw: string }[] = [];

  const now = () => new Date().toISOString().replace(/\.\d+Z$/, 'Z');
  const fresh = (): Step[] => KINDS.map((kind) => ({ kind, state: 'pending', detail: null }));
  const members = (r: Run) =>
    o.downloads().filter((d) => (r.packageId !== null ? d.packageId === r.packageId : d.id === r.downloadId));
  const nameOf = (r: Run) => {
    if (r.packageId !== null) return o.packages().find((p) => p.id === r.packageId)?.name ?? 'Package';
    const n = members(r)[0]?.name ?? 'download';
    return n.replace(/\.[^.]*$/, '') || n;
  };
  const touch = (r: MockRun, state?: RunState) => {
    if (state) r.state = state;
    r.updatedAt = now();
    o.changed();
  };

  function outcome(r: MockRun, kind: StepKind): [Step['state'], string] {
    const files = members(r);
    const name = nameOf(r);
    switch (kind) {
      case 'verify':
        return files.some((d) => hashes.has(d.id)) ? ['done', 'SHA-256 from your checksum'] : ['skipped', 'No checksum'];
      case 'extract': {
        if (!files.some((d) => ARCHIVE.test(d.name))) return ['skipped', 'Not an archive'];
        if (/damaged/i.test(name)) return ['failed', 'Archive is damaged: Cannot open the file as archive'];
        if (/secret/i.test(name)) {
          const given = r.given;
          r.given = null;
          const ok = given === 'haul' || passwords.some((p) => p.pw === 'haul');
          if (!ok) return ['needs_password', given === null ? 'Needs a password' : 'Wrong password'];
          if (given === 'haul' && r.remember && !passwords.some((p) => p.pw === 'haul'))
            passwords.push({ id: nextPw++, pw: 'haul' });
        }
        return ['done', `Extracted to ${DIR}\\${name}`];
      }
      case 'sort': {
        const first = files[0]?.name ?? name;
        const d = decide(first, rules, categories, settings.sortCategories);
        if (d.folder === null) return ['skipped', 'Nothing to move'];
        return ['done', `${d.by} → ${d.folder}`];
      }
      case 'scan':
        return settings.virusScan ? ['done', 'No threats'] : ['skipped', 'Off'];
      case 'notify':
        return ['done', 'Sent'];
    }
  }

  function advance(r: MockRun) {
    const running = r.steps.find((s) => s.state === 'running');
    if (!running) {
      const next = r.steps.find((s) => s.state === 'pending');
      if (!next) return touch(r, 'done');
      next.state = 'running';
      return touch(r, 'running');
    }
    const [state, detail] = outcome(r, running.kind);
    running.state = state;
    running.detail = detail;
    if (state === 'failed') return touch(r, 'failed');
    if (state === 'needs_password') return touch(r, 'needs_password');
    if (r.steps.every((s) => s.state === 'done' || s.state === 'skipped')) return touch(r, 'done');
    touch(r);
  }

  function requeue(r: MockRun) {
    for (const s of r.steps) if (s.state !== 'done' && s.state !== 'skipped') s.state = 'pending';
    touch(r, 'queued');
  }

  function tick() {
    const done = o.downloads().filter((d) => d.state === 'completed');
    const targets: { packageId: number | null; downloadId: number | null }[] = [];
    for (const p of o.packages()) {
      const all = o.downloads().filter((d) => d.packageId === p.id);
      if (all.length && all.every((d) => d.state === 'completed')) targets.push({ packageId: p.id, downloadId: null });
    }
    for (const d of done) if (d.packageId === null) targets.push({ packageId: null, downloadId: d.id });
    for (const t of targets) {
      if (runs.some((r) => r.packageId === t.packageId && r.downloadId === t.downloadId)) continue;
      const r: MockRun = {
        id: nextRun++, ...t, state: settings.auto ? 'queued' : 'held', steps: fresh(), updatedAt: now(),
        given: null, remember: false,
      };
      runs.push(r);
      o.changed();
    }
    // One run at a time, like the engine.
    const r = runs.find((x) => x.state === 'running') ?? runs.find((x) => x.state === 'queued');
    if (r) advance(r);
  }

  const find = (id: number) => {
    const r = runs.find((x) => x.id === id);
    if (!r) throw 'No such run';
    return r;
  };
  const view = (r: MockRun): Run => {
    const { given: _g, remember: _r, ...rest } = r;
    return structuredClone(rest);
  };

  return {
    tick,
    runs: () => runs.map(view),
    process(t: { packageId?: number; downloadId?: number }): number {
      const pkg = t.packageId ?? null;
      const dl = pkg === null ? (t.downloadId ?? null) : null;
      const files = o.downloads().filter((d) => (pkg !== null ? d.packageId === pkg : d.id === dl));
      if (!files.length || files.some((d) => d.state !== 'completed')) throw 'Not finished yet';
      let r = runs.find((x) => x.packageId === pkg && x.downloadId === dl);
      if (!r) {
        r = { id: nextRun++, packageId: pkg, downloadId: dl, state: 'queued', steps: fresh(), updatedAt: now(), given: null, remember: false };
        runs.push(r);
        o.changed();
      } else requeue(r);
      return r.id;
    },
    retry(id: number) {
      const r = find(id);
      if (!['failed', 'held', 'needs_password'].includes(r.state)) throw 'Nothing to retry';
      requeue(r);
    },
    password(id: number, pw: string, remember: boolean) {
      const r = find(id);
      r.given = pw;
      r.remember = remember;
      if (!['failed', 'held', 'needs_password'].includes(r.state)) throw 'Nothing to retry';
      requeue(r);
    },
    setHash(id: number, hash: string | null) {
      const h = hash?.trim().replace(/^(md5|sha1|sha256|crc32):/i, '') ?? '';
      if (!h) return void hashes.delete(id);
      if (!/^[0-9a-f]+$/i.test(h) || ![8, 32, 40, 64].includes(h.length))
        throw 'Not a checksum: use 8, 32, 40 or 64 hex characters';
      hashes.set(id, h.toLowerCase());
    },
    rulesConfig: (): RulesConfig =>
      structuredClone({ settings, categories, rules, passwords: passwords.map((p) => p.id) }),
    setSettings: (s: PostProcSettings) => void (settings = { ...s }),
    setCategories(c: Category[]) {
      if (!c.length) throw 'Keep at least one category';
      if (c.some((x) => !x.name.trim())) throw 'Every category needs a name';
      categories = structuredClone(c);
    },
    setRules(r: Rule[]) {
      validate(r);
      rules = structuredClone(r).map((x, i) => ({ ...x, id: i + 1 }));
    },
    tryRules(f: { name: string }, d: { rules: Rule[]; categories: Category[]; sort: boolean }): Destination {
      validate(d.rules);
      return decide(f.name, d.rules, d.categories, d.sort);
    },
    addPassword(pw: string): number {
      if (!pw) throw 'Type a password first';
      const id = nextPw++;
      passwords.push({ id, pw });
      return id;
    },
    removePassword: (id: number) => void (passwords = passwords.filter((p) => p.id !== id)),
    orderPasswords(ids: number[]) {
      passwords = ids.map((id) => passwords.find((p) => p.id === id)!).filter(Boolean);
    },
  };
}
