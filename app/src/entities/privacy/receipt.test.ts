import { describe, expect, it } from '@jest/globals';

import { describeReceipt, formatBytes } from './api';

const now = new Date('2026-11-18T12:00:00Z');

function receipt(daysAgo: number, bytes = 2048) {
  return { created_at: new Date(now.getTime() - daysAgo * 86_400_000).toISOString(), bytes };
}

describe('an export receipt in words', () => {
  it('holds a size a person can read', () => {
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(2048)).toBe('2 KB');
    expect(formatBytes(1_572_864)).toBe('1.5 MB');
  });

  it('says today and yesterday while those words are still true', () => {
    expect(describeReceipt(receipt(0), now)).toBe('2 KB, today');
    expect(describeReceipt(receipt(1), now)).toBe('2 KB, yesterday');
    expect(describeReceipt(receipt(3), now)).toBe('2 KB, 3 days ago');
  });

  it('becomes a date once "days ago" stops helping', () => {
    // A receipt from March is a date; "247 days ago" is arithmetic the customer has to do.
    expect(describeReceipt(receipt(247), now)).not.toContain('days ago');
  });
});
