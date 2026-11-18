import { conditionFor, conditionOptions } from './AutomationsPage';
// `describe as describeRule`: the entity's name for "what a rule does" is the same word Jest uses for a
// test group, and the import shadows the global.
import { describe as describeRule, describeCondition, type Automation } from '../../entities/automation';

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
      'Every 5 min, if someone is home → set switch.power',
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
    ).toBe('Every 5 min, if nobody is home and it is dark → set switch.power');
  });

  it('is unchanged for a rule with no conditions', () => {
    expect(describeRule(rule([]))).toBe('Every 5 min → set switch.power');
  });
});
