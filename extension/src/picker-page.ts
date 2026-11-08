import { KINDS, kindOf, uniqueLinks, type Kind } from './picker';

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const tab = new URLSearchParams(location.search).get('tab') ?? '';

interface Row {
  url: string;
  kind: Kind;
  checked: boolean;
}

let rows: Row[] = [];
let title = '';
// Every kind shown at first, except plain pages ("other"), which are rarely downloads.
const shown = new Set<Kind>(KINDS.map((k) => k.id).filter((k) => k !== 'other'));

function visible(): Row[] {
  return rows.filter((r) => shown.has(r.kind));
}

function renderChips() {
  const box = $('chips');
  box.replaceChildren();
  for (const k of KINDS) {
    const n = rows.filter((r) => r.kind === k.id).length;
    if (!n) continue;
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'chip';
    b.textContent = `${k.label} (${n})`;
    b.setAttribute('aria-pressed', String(shown.has(k.id)));
    b.addEventListener('click', () => {
      if (shown.has(k.id)) shown.delete(k.id);
      else shown.add(k.id);
      render();
    });
    box.append(b);
  }
}

function renderList() {
  const list = $('list');
  list.replaceChildren();
  const vis = visible();
  for (const r of vis) {
    const li = document.createElement('li');
    const label = document.createElement('label');
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.checked = r.checked;
    cb.addEventListener('change', () => {
      r.checked = cb.checked;
      renderFooter();
    });
    const kind = document.createElement('span');
    kind.className = 'kind';
    kind.textContent = KINDS.find((k) => k.id === r.kind)?.label ?? '';
    const url = document.createElement('span');
    url.className = 'url';
    url.textContent = r.url;
    url.title = r.url;
    label.append(cb, kind, url);
    li.append(label);
    list.append(li);
  }
  $('empty').hidden = vis.length > 0;
  const all = $<HTMLInputElement>('all');
  all.checked = vis.length > 0 && vis.every((r) => r.checked);
}

function renderFooter() {
  const n = visible().filter((r) => r.checked).length;
  const send = $<HTMLButtonElement>('send');
  send.disabled = n === 0;
  send.textContent = n ? `Send ${n} to Haul` : 'Send to Haul';
}

function render() {
  renderChips();
  renderList();
  renderFooter();
}

$('all').addEventListener('change', (e) => {
  const on = (e.target as HTMLInputElement).checked;
  for (const r of visible()) r.checked = on;
  render();
});

$('send').addEventListener('click', async () => {
  const links = visible().filter((r) => r.checked).map((r) => r.url);
  const note = $('note');
  $<HTMLButtonElement>('send').disabled = true;
  note.textContent = 'Sending…';
  const r = (await chrome.runtime.sendMessage({ type: 'send-links', links, pageTitle: title })) as {
    outcome: 'ok' | 'unpaired' | 'failed';
    added: number;
  };
  note.textContent =
    r.outcome === 'ok'
      ? `Sent ${r.added} ${r.added === 1 ? 'link' : 'links'} to Haul's Collector`
      : r.outcome === 'unpaired'
        ? 'Not paired: open the Haul button in the toolbar and press Pair'
        : "Haul didn't answer. Is it running?";
  renderFooter();
});

void chrome.storage.session.get(`picker:${tab}`).then((v) => {
  const found = v[`picker:${tab}`] as { links: string[]; title: string } | undefined;
  title = found?.title ?? '';
  $('page').textContent = title ? `From “${title}”` : '';
  rows = uniqueLinks(found?.links ?? []).map((url) => ({ url, kind: kindOf(url), checked: kindOf(url) !== 'other' }));
  render();
});
