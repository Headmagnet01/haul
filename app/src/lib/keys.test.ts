import { expect, test } from 'vitest';
import { keyAction } from './keys';

const k = (key: string, mods: Partial<{ altKey: boolean; ctrlKey: boolean; shiftKey: boolean }> = {}, tag = 'DIV') =>
  keyAction({ key, altKey: false, ctrlKey: false, shiftKey: false, ...mods, target: { tagName: tag } });

test('key map', () => {
  expect(k(' ')).toBe('toggle');
  expect(k('Delete')).toBe('remove');
  expect(k('ArrowDown')).toBe('down');
  expect(k('ArrowUp')).toBe('up');
  expect(k('ArrowUp', { altKey: true })).toBe('move-up');
  expect(k('ArrowDown', { altKey: true })).toBe('move-down');
  expect(k('ArrowRight')).toBe('expand');
  expect(k('ArrowLeft')).toBe('collapse');
  expect(k('f', { ctrlKey: true })).toBe('focus-filter');
  expect(k('a')).toBeNull();
  expect(k(' ', { ctrlKey: true })).toBeNull();
});

test('keys_ignored_in_text_fields', () => {
  expect(k(' ', {}, 'INPUT')).toBeNull();
  expect(k('Delete', {}, 'TEXTAREA')).toBeNull();
  expect(k('ArrowDown', {}, 'SELECT')).toBeNull();
  expect(
    keyAction({ key: ' ', altKey: false, ctrlKey: false, shiftKey: false, target: { tagName: 'DIV', isContentEditable: true } }),
  ).toBeNull();
  expect(keyAction({ key: ' ', altKey: false, ctrlKey: false, shiftKey: false, target: null })).toBe('toggle');
});
