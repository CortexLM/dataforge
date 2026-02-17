/**
 * Tests for Gemini yolo flag handling in consult command
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { tmpdir } from 'node:os';

// Mock child_process
vi.mock('node:child_process', () => ({
  spawn: vi.fn(() => ({
    stdout: {
      on: vi.fn(),
    },
    on: vi.fn((event: string, callback: (code: number) => void) => {
      if (event === 'close') callback(0);
    }),
  })),
  execSync: vi.fn((cmd: string) => {
    if (cmd.includes('which gemini')) {
      return Buffer.from('/usr/bin/gemini');
    }
    return Buffer.from('');
  }),
}));

// Mock chalk
vi.mock('chalk', () => ({
  default: {
    bold: (s: string) => s,
    green: (s: string) => s,
    yellow: (s: string) => s,
    red: (s: string) => s,
    blue: (s: string) => s,
    dim: (s: string) => s,
  },
}));

describe('Gemini yolo flag handling', () => {
  const testBaseDir = path.join(tmpdir(), `codev-consult-yolo-test-${Date.now()}`);
  let originalCwd: string;

  beforeEach(() => {
    originalCwd = process.cwd();
    fs.mkdirSync(testBaseDir, { recursive: true });
    vi.spyOn(console, 'error').mockImplementation(() => {});
  });

  afterEach(() => {
    process.chdir(originalCwd);
    vi.restoreAllMocks();
    if (fs.existsSync(testBaseDir)) {
      fs.rmSync(testBaseDir, { recursive: true });
    }
  });

  it('general mode should not pass --yolo to Gemini', async () => {
    vi.resetModules();

    fs.mkdirSync(path.join(testBaseDir, 'codev', 'roles'), { recursive: true });
    fs.writeFileSync(
      path.join(testBaseDir, 'codev', 'roles', 'consultant.md'),
      '# Consultant Role'
    );

    process.chdir(testBaseDir);

    const { spawn } = await import('node:child_process');
    vi.mocked(spawn).mockClear();
    const { consult } = await import('../commands/consult/index.js');

    const prompt = 'summarize risk hotspots in the repo';
    await consult({ model: 'gemini', prompt });

    const spawnCalls = vi.mocked(spawn).mock.calls;
    const geminiCall = spawnCalls.find(call => call[0] === 'gemini');
    expect(geminiCall).toBeDefined();

    const args = geminiCall![1] as string[];
    expect(args).not.toContain('--yolo');
    expect(args[args.length - 1]).toContain(prompt);
  });

  it('protocol mode should pass --yolo to Gemini', async () => {
    vi.resetModules();

    fs.mkdirSync(path.join(testBaseDir, 'codev', 'roles'), { recursive: true });
    fs.mkdirSync(path.join(testBaseDir, 'codev', 'protocols', 'rapid', 'consult-types'), { recursive: true });
    fs.mkdirSync(path.join(testBaseDir, 'codev', 'specs'), { recursive: true });

    fs.writeFileSync(
      path.join(testBaseDir, 'codev', 'roles', 'consultant.md'),
      '# Consultant Role'
    );
    fs.writeFileSync(
      path.join(testBaseDir, 'codev', 'protocols', 'rapid', 'consult-types', 'spec-review.md'),
      '# Rapid review instructions'
    );
    fs.writeFileSync(
      path.join(testBaseDir, 'codev', 'specs', '0007-bright-idea.md'),
      '# Bright Idea Spec'
    );

    process.chdir(testBaseDir);

    const { spawn } = await import('node:child_process');
    vi.mocked(spawn).mockClear();
    const { consult } = await import('../commands/consult/index.js');

    await consult({ model: 'gemini', protocol: 'rapid', type: 'spec', issue: '7' });

    const spawnCalls = vi.mocked(spawn).mock.calls;
    const geminiCall = spawnCalls.find(call => call[0] === 'gemini');
    expect(geminiCall).toBeDefined();

    const args = geminiCall![1] as string[];
    expect(args).toContain('--yolo');
    expect(args[args.length - 1]).toContain('Review Specification');
  });
});
