import { describeUpdate, type DeviceUpdate } from './updates';

const base: DeviceUpdate = {
  current: '1.0.0',
  target: '2.0.0',
  state: null,
  error: null,
  at: null,
  in_progress: false,
};

describe('describeUpdate', () => {
  it('says nothing when nothing has ever been attempted', () => {
    // A device nobody has tried to update needs no line at all.
    expect(describeUpdate(undefined)).toBeNull();
    expect(describeUpdate(base)).toBeNull();
  });

  it('names the version while it is updating', () => {
    expect(describeUpdate({ ...base, state: 'downloading', in_progress: true })?.text).toBe(
      'Updating to 2.0.0…',
    );
  });

  it('gives the reason, not just the failure', () => {
    // "Update failed" alone is a support call; the reason is what makes it
    // actionable.
    const out = describeUpdate({
      ...base,
      state: 'failed',
      error: 'no progress after 45 minutes',
    });
    expect(out?.bad).toBe(true);
    expect(out?.text).toBe('Update to 2.0.0 failed: no progress after 45 minutes');
  });

  it('still says something when a failure carries no reason', () => {
    const out = describeUpdate({ ...base, state: 'failed' });
    expect(out?.text).toBe('Update to 2.0.0 failed');
  });

  it('reports a skipped update plainly', () => {
    expect(describeUpdate({ ...base, state: 'skipped' })?.text).toBe(
      'Update to 2.0.0 was skipped',
    );
  });
});
