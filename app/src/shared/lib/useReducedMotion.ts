import { useEffect, useState } from 'react';
import { AccessibilityInfo } from 'react-native';

/**
 * May this app animate?
 *
 * `Reduce Motion` is a promise the platform makes on the customer's behalf, and honouring it is not decoration: a
 * sweep, a pulse and a sliding pill are all vestibular triggers. One hook answers the question so a component does not
 * have to remember the async settings read, the subscription, and the unsubscribe — three things that get skipped.
 *
 * Defaults to **false** (animate) until the setting is known, because a component that renders static on its first
 * frame and then starts moving is worse than one that starts still and stops.
 */
export function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(false);

  useEffect(() => {
    let cancelled = false;

    void AccessibilityInfo.isReduceMotionEnabled().then((value) => {
      if (!cancelled) setReduced(value);
    });

    const subscription = AccessibilityInfo.addEventListener('reduceMotionChanged', (value) => {
      setReduced(value);
    });

    return () => {
      cancelled = true;
      subscription.remove();
    };
  }, []);

  return reduced;
}
