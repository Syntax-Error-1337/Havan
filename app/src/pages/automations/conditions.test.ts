import { conditionFor, conditionOptions } from './AutomationsPage';
// `describe as describeRule`: the entity's name for "what a rule does" is the same word Jest uses for a
// test group, and the import shadows the global.
import { describe as describeRule, describeCondition, humanEvery, type Automation } from '../../entities/automation';

/**
 * The "Only if" half of a rule.
 *
 * The engine has evaluated conditions since the first version and the editor sent `conditions: []`, so every rule
 * a customer built ran unconditionally and the sentence "…but only when someone is home" was a curl call.
 */

describe('conditionFor', () => {
  it('sends nothing when the rule should run any time', () => {
    expect(conditionFor('any')).toEqual([]);
  });

  it('sends the engine’s own shape for presence and daylight', () => {
    expect(conditionFor('home')).toEqual([{ type: 'present', presence: 'home' }]);
    expect(conditionFor('away')).toEqual([{ type: 'present', presence: 'away' }]);
    expect(conditionFor('sunset')).toEqual([{ type: 'solar', event: 'sunset', offset_min: 0 }]);
    expect(conditionFor('sunrise')).toEqual([{ type: 'solar', event: 'sunrise', offset_min: 0 }]);
  });
});

describe('conditionOptions', () => {
  it('offers sun conditions only where there is a location to compute them from', () => {
    // With no coordinates the engine's Solar condition is `false` forever, so offering it would be a control that
    // can only fail — the customer would decide the automation was broken.
    const withoutLocation = conditionOptions(false).map((o) => o.id);
    // A time window needs no coordinates — it is the home's own clock — so it is offered on both homes.
    expect(withoutLocation).toEqual(['any', 'home', 'away', 'window']);

    const withLocation = conditionOptions(true).map((o) => o.id);
    expect(withLocation).toContain('sunset');
    expect(withLocation).toContain('sunrise');
  });
});

describe('describe with a condition', () => {
  const rule = (conditions: unknown[]): Automation =>
    ({
      id: 'r1',
      home_id: 'h1',
      name: 'Porch light',
      enabled: true,
      definition: {
        trigger: { type: 'schedule', every_secs: 300 },
        conditions,
        actions: [{ type: 'set_device', device_id: 'd1', code: 'switch.power', value: true }],
      },
      next_run_at: null,
    }) as unknown as Automation;

  it('says the condition out loud, because the list is where a rule is debugged', () => {
    expect(describeRule(rule([{ type: 'present', presence: 'home' }]))).toBe(
      // "turn the device on", not "set switch.power": the point code is somebody else's firmware talking.
      'Every 5 min, if someone is home → turn the device on',
    );
  });

  it('reads daylight in the customer’s words, not the engine’s', () => {
    expect(describeCondition({ type: 'solar', event: 'sunset', offset_min: 0 })).toBe('it is dark');
    expect(describeCondition({ type: 'solar', event: 'sunrise', offset_min: 0 })).toBe('it is light');
  });

  it('joins two conditions with "and"', () => {
    expect(
      describeRule(
        rule([
          { type: 'present', presence: 'away' },
          { type: 'solar', event: 'sunset', offset_min: 0 },
        ]),
      ),
    ).toBe('Every 5 min, if nobody is home and it is dark → turn the device on');
  });

  it('is unchanged for a rule with no conditions', () => {
    expect(describeRule(rule([]))).toBe('Every 5 min → turn the device on');
  });
});

describe('the one-line description', () => {
  const rule = (every: number, value: unknown = true) =>
    ({
      id: 'a1',
      home_id: 'h1',
      name: 'Porch light',
      enabled: true,
      next_run_at: null,
      definition: {
        trigger: { type: 'schedule', every_secs: every },
        conditions: [],
        actions: [{ type: 'set_device', device_id: 'd1', code: 'switch.power', value }],
      },
    }) as never;

  it('says a daily rule daily, not 1440 minutes', () => {
    // The review read "Every 1440 min" on a rule called "Porch light at dusk" and asked which was wrong — the number
    // was, and it was ours.
    expect(humanEvery(86_400)).toBe('Every day');
    expect(humanEvery(172_800)).toBe('Every 2 days');
    expect(humanEvery(3_600)).toBe('Every hour');
    expect(humanEvery(10_800)).toBe('Every 3 hours');
    expect(humanEvery(300)).toBe('Every 5 min');
    // Below a minute the seconds are the honest unit.
    expect(humanEvery(30)).toBe('Every 30s');
  });

  it('names the device, not the point code', () => {
    const names = (id: string) => (id === 'd1' ? 'Porch light' : undefined);
    // `describeRule`, not `describe`: in a test file `describe` is jest's own.
    expect(describeRule(rule(86_400, true), names)).toContain('turn Porch light on');
    expect(describeRule(rule(86_400, false), names)).toContain('turn Porch light off');
    // With no name to hand it still reads as a sentence about a device rather than "set switch.power".
    expect(describeRule(rule(86_400, true))).toContain('turn the device on');
  });
});
