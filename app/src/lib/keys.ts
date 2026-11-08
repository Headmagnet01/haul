export type Action = 'up' | 'down' | 'toggle' | 'remove' | 'expand' | 'collapse' | 'move-up' | 'move-down' | 'focus-filter'
  | 'open' | 'delete-file';

export interface KeyLike {
  key: string;
  altKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  target: { tagName: string; isContentEditable?: boolean } | null;
}

const TEXT_TAGS = new Set(['INPUT', 'TEXTAREA', 'SELECT']);

/** Queue and Finished shortcuts. Nothing fires while typing in a text field, so Space types a space there. */
export function keyAction(e: KeyLike): Action | null {
  if (e.target && (TEXT_TAGS.has(e.target.tagName.toUpperCase()) || e.target.isContentEditable)) return null;
  if (e.ctrlKey) return !e.altKey && e.key.toLowerCase() === 'f' ? 'focus-filter' : null;
  if (e.altKey) {
    if (e.key === 'ArrowUp') return 'move-up';
    if (e.key === 'ArrowDown') return 'move-down';
    return null;
  }
  switch (e.key) {
    case ' ':
      return 'toggle';
    case 'Delete':
      return e.shiftKey ? 'delete-file' : 'remove';
    case 'Enter':
      return 'open';
    case 'ArrowUp':
      return 'up';
    case 'ArrowDown':
      return 'down';
    case 'ArrowRight':
      return 'expand';
    case 'ArrowLeft':
      return 'collapse';
    default:
      return null;
  }
}
