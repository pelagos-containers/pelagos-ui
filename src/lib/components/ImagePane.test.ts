import { describe, it, expect } from 'vitest';
import type { ImageInfo } from '$lib/ipc';

// ── helpers extracted from ImagePane (pure functions, no Tauri) ─────────────

function shortDigest(digest: string): string {
  const hex = digest.startsWith('sha256:') ? digest.slice(7) : digest;
  return hex.slice(0, 12);
}

function filterImages(images: ImageInfo[], query: string) {
  const q = query.trim().toLowerCase();
  const matchedImages = q
    ? images.filter(img => img.reference.toLowerCase().includes(q))
    : [...images];
  const exactMatch = images.some(img => img.reference.toLowerCase() === q);
  const showNewEntry = q !== '' && !exactMatch;
  return { matchedImages, exactMatch, showNewEntry };
}

// ── test fixtures ────────────────────────────────────────────────────────────

const IMAGES: ImageInfo[] = [
  { reference: 'alpine:latest',          digest: 'sha256:abcdef123456789012', layers: ['a', 'b'] },
  { reference: 'nginx:alpine',           digest: 'sha256:fedcba987654321098', layers: ['a'] },
  { reference: 'debian:bookworm-slim',   digest: 'sha256:112233445566778899', layers: ['a', 'b', 'c'] },
  { reference: 'public.ecr.aws/pelagos/pelagos:latest', digest: 'sha256:aabbccddeeff001122', layers: [] },
];

// ── shortDigest ──────────────────────────────────────────────────────────────

describe('shortDigest', () => {
  it('strips sha256: prefix and returns 12 chars', () => {
    expect(shortDigest('sha256:abcdef123456789012')).toBe('abcdef123456');
  });

  it('returns 12 chars when no prefix', () => {
    expect(shortDigest('abcdef123456789012')).toBe('abcdef123456');
  });

  it('handles digest shorter than 12 chars gracefully', () => {
    expect(shortDigest('sha256:abc')).toBe('abc');
  });

  it('handles empty string', () => {
    expect(shortDigest('')).toBe('');
  });
});

// ── filterImages ─────────────────────────────────────────────────────────────

describe('filterImages — empty query', () => {
  it('returns all images', () => {
    const { matchedImages } = filterImages(IMAGES, '');
    expect(matchedImages).toHaveLength(IMAGES.length);
  });

  it('showNewEntry is false', () => {
    const { showNewEntry } = filterImages(IMAGES, '');
    expect(showNewEntry).toBe(false);
  });

  it('exactMatch is false', () => {
    const { exactMatch } = filterImages(IMAGES, '');
    expect(exactMatch).toBe(false);
  });
});

describe('filterImages — partial match', () => {
  it('filters to images containing the query', () => {
    const { matchedImages } = filterImages(IMAGES, 'alpine');
    expect(matchedImages.map(i => i.reference)).toEqual(['alpine:latest', 'nginx:alpine']);
  });

  it('showNewEntry is true when no exact match', () => {
    const { showNewEntry } = filterImages(IMAGES, 'alpine');
    expect(showNewEntry).toBe(true);
  });

  it('matching is case-insensitive', () => {
    const { matchedImages } = filterImages(IMAGES, 'ALPINE');
    expect(matchedImages).toHaveLength(2);
  });

  it('matches on full reference including registry', () => {
    const { matchedImages } = filterImages(IMAGES, 'ecr.aws');
    expect(matchedImages).toHaveLength(1);
    expect(matchedImages[0].reference).toBe('public.ecr.aws/pelagos/pelagos:latest');
  });
});

describe('filterImages — exact match', () => {
  it('exactMatch is true when query equals a reference exactly', () => {
    const { exactMatch } = filterImages(IMAGES, 'alpine:latest');
    expect(exactMatch).toBe(true);
  });

  it('showNewEntry is false on exact match', () => {
    const { showNewEntry } = filterImages(IMAGES, 'alpine:latest');
    expect(showNewEntry).toBe(false);
  });

  it('exact match is case-insensitive', () => {
    const { exactMatch } = filterImages(IMAGES, 'ALPINE:LATEST');
    expect(exactMatch).toBe(true);
  });
});

describe('filterImages — no match', () => {
  it('returns empty matchedImages', () => {
    const { matchedImages } = filterImages(IMAGES, 'ubuntu:22.04');
    expect(matchedImages).toHaveLength(0);
  });

  it('showNewEntry is true', () => {
    const { showNewEntry } = filterImages(IMAGES, 'ubuntu:22.04');
    expect(showNewEntry).toBe(true);
  });
});

describe('filterImages — whitespace handling', () => {
  it('trims leading/trailing whitespace from query', () => {
    const { matchedImages } = filterImages(IMAGES, '  alpine  ');
    expect(matchedImages).toHaveLength(2);
  });

  it('whitespace-only query treated as empty', () => {
    const { showNewEntry, matchedImages } = filterImages(IMAGES, '   ');
    expect(showNewEntry).toBe(false);
    expect(matchedImages).toHaveLength(IMAGES.length);
  });
});

describe('filterImages — empty image list', () => {
  it('returns empty matchedImages with a query', () => {
    const { matchedImages, showNewEntry } = filterImages([], 'alpine');
    expect(matchedImages).toHaveLength(0);
    expect(showNewEntry).toBe(true);
  });

  it('returns empty matchedImages with no query', () => {
    const { matchedImages, showNewEntry } = filterImages([], '');
    expect(matchedImages).toHaveLength(0);
    expect(showNewEntry).toBe(false);
  });
});
