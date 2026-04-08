import { describe, it, expect } from 'vitest';
import type { ContainerInfo } from '$lib/ipc';

// ── helpers mirroring +page.svelte selection logic ───────────────────────────

function initSelection(containers: ContainerInfo[]): Set<string> {
  return new Set(containers.filter(c => c.status === 'exited').map(c => c.name));
}

function toggleSelected(selected: Set<string>, name: string): Set<string> {
  const next = new Set(selected);
  if (next.has(name)) next.delete(name); else next.add(name);
  return next;
}

// ── fixtures ─────────────────────────────────────────────────────────────────

function makeContainer(name: string, status: 'running' | 'exited'): ContainerInfo {
  return {
    name,
    status,
    pid: status === 'running' ? 123 : 0,
    started_at: new Date().toISOString(),
    rootfs: '',
    command: ['/bin/sh'],
    image: 'alpine:latest',
  };
}

const RUNNING = makeContainer('web', 'running');
const EXITED1 = makeContainer('db', 'exited');
const EXITED2 = makeContainer('cache', 'exited');
const ALL = [RUNNING, EXITED1, EXITED2];

// ── initSelection ────────────────────────────────────────────────────────────

describe('initSelection', () => {
  it('pre-selects all exited containers', () => {
    const sel = initSelection(ALL);
    expect(sel.has('db')).toBe(true);
    expect(sel.has('cache')).toBe(true);
  });

  it('does not select running containers', () => {
    const sel = initSelection(ALL);
    expect(sel.has('web')).toBe(false);
  });

  it('returns empty set when no exited containers', () => {
    const sel = initSelection([RUNNING]);
    expect(sel.size).toBe(0);
  });

  it('returns empty set for empty list', () => {
    expect(initSelection([])).toEqual(new Set());
  });

  it('selects all when all are exited', () => {
    const sel = initSelection([EXITED1, EXITED2]);
    expect(sel.size).toBe(2);
  });
});

// ── toggleSelected ────────────────────────────────────────────────────────────

describe('toggleSelected', () => {
  it('adds a name that is not selected', () => {
    const sel = toggleSelected(new Set(['db']), 'cache');
    expect(sel.has('cache')).toBe(true);
    expect(sel.has('db')).toBe(true);
  });

  it('removes a name that is already selected', () => {
    const sel = toggleSelected(new Set(['db', 'cache']), 'db');
    expect(sel.has('db')).toBe(false);
    expect(sel.has('cache')).toBe(true);
  });

  it('does not mutate the original set', () => {
    const original = new Set(['db']);
    toggleSelected(original, 'cache');
    expect(original.size).toBe(1);
  });

  it('toggling the same name twice restores original state', () => {
    const start = new Set(['db']);
    const after = toggleSelected(toggleSelected(start, 'cache'), 'cache');
    expect(after).toEqual(start);
  });

  it('toggle on empty set adds the name', () => {
    const sel = toggleSelected(new Set(), 'db');
    expect(sel).toEqual(new Set(['db']));
  });

  it('toggle sole member produces empty set', () => {
    const sel = toggleSelected(new Set(['db']), 'db');
    expect(sel.size).toBe(0);
  });
});

// ── remove-btn disabled logic ─────────────────────────────────────────────────

describe('Remove selected button disabled state', () => {
  it('disabled when selection is empty', () => {
    const disabled = new Set().size === 0;
    expect(disabled).toBe(true);
  });

  it('enabled when at least one item selected', () => {
    const disabled = new Set(['db']).size === 0;
    expect(disabled).toBe(false);
  });
});
