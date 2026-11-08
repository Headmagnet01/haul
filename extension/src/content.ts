// Tells the worker about Alt+clicks on links, so those downloads stay in the browser.
document.addEventListener(
  'click',
  (e) => {
    if (!e.altKey) return;
    const a = (e.target as Element | null)?.closest?.('a[href]') as HTMLAnchorElement | null;
    if (!a?.href) return;
    try {
      void chrome.runtime.sendMessage({ type: 'alt-click', url: a.href });
    } catch {
      // the extension was reloaded; this page's script is stale
    }
  },
  true,
);
