import { describe, expect, it } from 'vitest';
import { formatBytes, formatSpeed, formatDuration, hostOf, formatPercent, baseName } from './format';

describe('formatBytes', () => {
  it('is zero-safe and non-finite safe', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(-3)).toBe('0 B');
    expect(formatBytes(Number.NaN)).toBe('0 B');
    expect(formatBytes(Number.POSITIVE_INFINITY)).toBe('0 B');
  });
  it('counts kilobytes with binary arithmetic', () => {
    expect(formatBytes(1024)).toBe('1.0 KB');
    expect(formatBytes(1024 * 1024)).toBe('1.0 MB');
    expect(formatBytes(1024 * 1024 * 512)).toBe('512 MB');
  });
  it('large values get zero decimals', () => {
    expect(formatBytes(1024 * 1024 * 1024 * 3)).toBe('3.0 GB');
  });
});

describe('formatSpeed', () => {
  it('is zero-safe', () => {
    expect(formatSpeed(0)).toBe('—');
    expect(formatSpeed(-1)).toBe('—');
  });
  it('formats bytes per second', () => {
    expect(formatSpeed(1024)).toBe('1.0 KB/s');
    expect(formatSpeed(15 * 1024 * 1024)).toBe('15.0 MB/s');
  });
});

describe('formatDuration', () => {
  it('returns dash on junk', () => {
    expect(formatDuration(0)).toBe('—');
    expect(formatDuration(Number.NaN)).toBe('—');
  });
  it('m:ss under an hour', () => {
    expect(formatDuration(65)).toBe('1:05');
    expect(formatDuration(599)).toBe('9:59');
  });
  it('h:mm:ss over an hour', () => {
    expect(formatDuration(3600)).toBe('1:00:00');
    expect(formatDuration(3661)).toBe('1:01:01');
  });
});

describe('formatPercent', () => {
  it('clamps to 0-100 and keeps one decimal under 10', () => {
    expect(formatPercent(-5)).toBe('0.0%');
    expect(formatPercent(150)).toBe('100%');
    expect(formatPercent(4.25)).toBe('4.3%');
  });
});

describe('hostOf', () => {
  it('strips www and keeps the rest', () => {
    expect(hostOf('https://www.youtube.com/watch?v=x')).toBe('youtube.com');
    expect(hostOf('https://cdn1.example.net/file.zip')).toBe('cdn1.example.net');
  });
  it('invalid input never throws', () => {
    expect(hostOf('not a url')).toBe('unknown');
    expect(hostOf('')).toBe('unknown');
  });
});

describe('baseName', () => {
  it('handles posix and windows paths', () => {
    expect(baseName('/home/u/file.zip')).toBe('file.zip');
    expect(baseName('C:\\Users\\x\\file.exe')).toBe('file.exe');
    expect(baseName('file.bin')).toBe('file.bin');
  });
});
