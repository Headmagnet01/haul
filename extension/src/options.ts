import { loadOptions, saveOptions } from './store';

const MB = 1048576;
const minSize = document.getElementById('minSize') as HTMLInputElement;
const types = document.getElementById('types') as HTMLInputElement;
const savedNote = document.getElementById('saved') as HTMLElement;

void loadOptions().then((o) => {
  minSize.value = String(Math.round((o.minSize / MB) * 10) / 10);
  types.value = o.types.join(', ');
});

document.getElementById('form')!.addEventListener('submit', async (e) => {
  e.preventDefault();
  const mb = Number(minSize.value);
  const list = types.value
    .split(/[\s,]+/)
    .map((t) => t.trim().replace(/^\./, '').toLowerCase())
    .filter((t, i, all) => t && all.indexOf(t) === i);
  await saveOptions({ minSize: Number.isFinite(mb) && mb >= 0 ? Math.round(mb * MB) : MB, types: list });
  savedNote.textContent = 'Saved';
  setTimeout(() => (savedNote.textContent = ''), 2000);
});
