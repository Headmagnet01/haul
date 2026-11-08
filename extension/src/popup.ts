import type { Status } from './api';
import { lastResult, pairState } from './store';

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

async function render() {
  const [status, pair, last] = await Promise.all([
    chrome.runtime.sendMessage({ type: 'status' }) as Promise<Status>,
    pairState(),
    lastResult(),
  ]);
  const dot = $('dot');
  const state = $('state');
  const detail = $('detail');
  const pairBtn = $<HTMLButtonElement>('pair');
  dot.className = 'dot';
  if (!status.running) {
    state.textContent = "Haul isn't running";
    detail.textContent = 'Downloads stay in your browser until Haul is open.';
    dot.classList.add('err');
    pairBtn.hidden = true;
  } else if (status.paired) {
    state.textContent = 'Paired with Haul';
    detail.textContent = status.active === 1 ? '1 download active' : `${status.active} downloads active`;
    dot.classList.add('ok');
    pairBtn.hidden = true;
  } else {
    state.textContent = 'Not paired';
    detail.textContent =
      pair === 'waiting'
        ? 'Answer the question in Haul’s window.'
        : pair === 'denied'
          ? 'Haul said no. Press Pair to ask again.'
          : 'Press Pair, then choose Allow in Haul.';
    pairBtn.hidden = false;
    pairBtn.disabled = pair === 'waiting';
  }
  const lastEl = $('last');
  lastEl.hidden = !last;
  if (last) lastEl.textContent = `Last: ${last.text} (${new Date(last.at).toLocaleTimeString()})`;
}

$('pair').addEventListener('click', async () => {
  await chrome.runtime.sendMessage({ type: 'pair' });
  await render();
});
$('options').addEventListener('click', () => chrome.runtime.openOptionsPage());
chrome.storage.onChanged.addListener(() => void render());
void render();
