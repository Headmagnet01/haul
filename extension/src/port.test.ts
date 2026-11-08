import { expect, test } from 'vitest';
import { PORTS, findPort } from './port';
import type { Fetcher } from './api';

/** A fake network: `answers` maps a port to the status JSON it returns; other ports refuse. */
function net(answers: Record<number, unknown>) {
  const calls: string[] = [];
  const fetch: Fetcher = async (url, init) => {
    calls.push(url);
    if (init.method !== 'POST') throw new Error(`status must be a POST (Chrome drops Origin on GET), got ${init.method}`);
    const port = Number(new URL(url).port);
    if (!(port in answers)) throw new TypeError('Failed to fetch');
    return { status: 200, json: async () => answers[port] };
  };
  return { fetch, calls };
}

test('ports_are_17654_to_17664', () => {
  expect(PORTS[0]).toBe(17654);
  expect(PORTS.at(-1)).toBe(17664);
  expect(PORTS).toHaveLength(11);
});

test('uses_last_port_first', async () => {
  const n = net({ 17660: { app: 'haul' } });
  expect(await findPort(n.fetch, 17660)).toBe(17660);
  expect(n.calls).toEqual(['http://127.0.0.1:17660/v1/status']);
});

test('scans_in_order', async () => {
  const n = net({ 17656: { app: 'haul' }, 17658: { app: 'haul' } });
  expect(await findPort(n.fetch, null)).toBe(17656);
  expect(n.calls.map((u) => new URL(u).port)).toEqual(['17654', '17655', '17656']);
});

test('ignores_non_haul_answers', async () => {
  const n = net({ 17654: { app: 'other' }, 17655: 'nonsense', 17657: { app: 'haul' } });
  expect(await findPort(n.fetch, null)).toBe(17657);
});

test('null_when_none_answer', async () => {
  expect(await findPort(net({}).fetch, 17654)).toBeNull();
});
