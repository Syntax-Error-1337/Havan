import { clockValue, conditionFor } from './AutomationsPage';

/**
 * The window condition, which the engine has always accepted and the editor never offered.
 *
 * "Only between 22:00 and 06:00" is the second most common rule after "at sunset" — and a rule that mutates a device
 * on a schedule without a window is how a hallway light comes on at noon while somebody is at work.
 */
describe('a rule that only runs inside a time window', () => {
  it('accepts 24-hour times and refuses everything else', () => {
    expect(clockValue('22:00')).toBe('22:00');
    // One digit is still a time: a person typing "9:30" means nine thirty.
    expect(clockValue('9:30')).toBe('09:30');
    expect(clockValue(' 23:59 ')).toBe('23:59');
    // The ones that must not reach the engine as a string it will never match.
    expect(clockValue('24:00')).toBeNull();
    expect(clockValue('7')).toBeNull();
    expect(clockValue('22:60')).toBeNull();
    expect(clockValue('')).toBeNull();
  });

  it('sends a time_of_day condition, and nothing at all when the window is half-built', () => {
    expect(conditionFor('window', { after: '22:00', before: '06:00' })).toEqual([
      { type: 'time_of_day', after: '22:00', before: '06:00' },
    ]);
    // A chip with an unparseable field must not produce a condition that reads as true-forever.
    expect(conditionFor('window')).toEqual([]);
  });

  it('keeps the conditions it already had', () => {
    expect(conditionFor('any')).toEqual([]);
    expect(conditionFor('home')).toEqual([{ type: 'present', presence: 'home' }]);
    expect(conditionFor('sunset')).toEqual([{ type: 'solar', event: 'sunset', offset_min: 0 }]);
  });
});
