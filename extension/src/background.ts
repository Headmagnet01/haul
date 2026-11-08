// The service worker: catches downloads, runs the right-click menu and talks to Haul.
import { makeClient } from './api';
import { browserName, type RuntimeLike } from './browser';
import { cookieHeader, type CookieApi, type CookieSource } from './cookies';
import { decide, factsFromItem } from './decide';
import { handOver } from './handover';
import { realFetcher } from './net';
import { WORDS, addAltClick, altClicks, loadOptions, saved, setLastResult, setPairState } from './store';
import type { CaptureBody } from './types';

const haul = makeClient(realFetcher, saved);

// Alt+clicks also kept in memory: the click's message can land a moment before the download starts.
const recentAlt: { url: string; at: number }[] = [];

// Firefox-only fields that @types/chrome doesn't know.
type FirefoxExtras = { cookieStoreId?: string };
const cookiesApi = chrome.cookies as unknown as CookieApi;
const runtime = chrome.runtime as unknown as RuntimeLike;

function baseName(path: string): string | undefined {
  const n = path.split(/[\\/]/).pop();
  return n || undefined;
}

async function captureBody(
  url: string,
  src: CookieSource,
  filename?: string,
  size?: number,
): Promise<CaptureBody> {
  return {
    url,
    filename,
    size,
    referrer: src.referrer || undefined,
    user_agent: navigator.userAgent,
    cookies: await cookieHeader(cookiesApi, url, src),
  };
}

const downloadsApi = {
  pause: (id: number) => chrome.downloads.pause(id),
  resume: (id: number) => chrome.downloads.resume(id),
  cancel: (id: number) => chrome.downloads.cancel(id),
  erase: async (id: number) => void (await chrome.downloads.erase({ id })),
};

chrome.downloads.onCreated.addListener(async (item) => {
  if (item.state !== 'in_progress') return;
  const facts = factsFromItem(item);
  const alt = [...recentAlt, ...(await altClicks())];
  if (decide(facts, await loadOptions(), alt, Date.now()) !== 'catch') return;
  const src = { cookieStoreId: (item as FirefoxExtras).cookieStoreId, referrer: item.referrer };
  const body = await captureBody(facts.finalUrl, src, baseName(item.filename), facts.size ?? undefined);
  await handOver(item.id, body, downloadsApi, {
    capture: async (b) => {
      const out = await haul.capture(b);
      if (out === 'unpaired') await setPairState('idle');
      await setLastResult(out === 'ok' ? WORDS.sent : out === 'unpaired' ? WORDS.unpaired : WORDS.kept);
      return out;
    },
  });
});

function notify(message: string) {
  chrome.notifications.create({ type: 'basic', iconUrl: 'icons/haul-128.png', title: 'Haul', message });
}

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({ id: 'haul-link', title: 'Download with Haul', contexts: ['link'] });
  chrome.contextMenus.create({ id: 'haul-all', title: 'Download all links…', contexts: ['page', 'selection', 'link'] });
});

/** Runs in the page: every link, or only those inside the current selection. */
function collectLinks(): { links: string[]; title: string } {
  const sel = window.getSelection();
  let anchors = Array.from(document.querySelectorAll<HTMLAnchorElement>('a[href]'));
  if (sel && !sel.isCollapsed && sel.rangeCount) {
    const range = sel.getRangeAt(0);
    const inside = anchors.filter((a) => range.intersectsNode(a));
    if (inside.length) anchors = inside;
  }
  return { links: anchors.map((a) => a.href), title: document.title };
}

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  if (info.menuItemId === 'haul-link' && info.linkUrl) {
    const src = { cookieStoreId: (tab as FirefoxExtras | undefined)?.cookieStoreId, referrer: info.pageUrl ?? tab?.url };
    const out = await haul.capture(await captureBody(info.linkUrl, src));
    const text = out === 'ok' ? WORDS.sent : out === 'unpaired' ? WORDS.unpaired : "Haul didn't answer. Is it running?";
    await setLastResult(text);
    if (out !== 'ok') notify(text);
    return;
  }
  if (info.menuItemId === 'haul-all' && tab?.id !== undefined) {
    let found = { links: [] as string[], title: tab.title ?? '' };
    try {
      const [res] = await chrome.scripting.executeScript({ target: { tabId: tab.id }, func: collectLinks });
      if (res?.result) found = res.result;
    } catch {
      notify("Haul can't read the links on this page.");
      return;
    }
    await chrome.storage.session.set({ [`picker:${tab.id}`]: found });
    await chrome.tabs.create({ url: chrome.runtime.getURL(`picker.html?tab=${tab.id}`) });
  }
});

type Msg =
  | { type: 'alt-click'; url: string }
  | { type: 'pair' }
  | { type: 'status' }
  | { type: 'send-links'; links: string[]; pageTitle: string };

chrome.runtime.onMessage.addListener((msg: Msg, _sender, reply) => {
  switch (msg.type) {
    case 'alt-click': {
      const now = Date.now();
      recentAlt.splice(0, recentAlt.length, ...recentAlt.filter((a) => now - a.at < 10_000), { url: msg.url, at: now });
      void addAltClick(msg.url);
      return false;
    }
    case 'pair':
      // The popup closes when Haul's window takes focus; the worker waits for the answer.
      void (async () => {
        await setPairState('waiting');
        const r = await haul.pair(await browserName(runtime, navigator));
        await setPairState(r);
      })();
      reply({ ok: true });
      return false;
    case 'status':
      void haul.status().then(reply);
      return true;
    case 'send-links':
      void haul.links(msg.links, msg.pageTitle).then(reply);
      return true;
  }
});
