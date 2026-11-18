import { DISMISS_AT, isHorizontalSwipe, shouldDismiss } from './SwipeToDismiss';

/**
 * The two rules a swipe has to obey, tested as arithmetic rather than as a gesture.
 *
 * The gesture itself is not where the bugs live: they live in the threshold (a drag that is too short fires, or a flick
 * that is obviously a dismissal does not) and in the direction test (every scroll on the feed becomes a dismissal).
 */
describe('swiping a row away', () => {
  it('dismisses at the threshold and not one point short of it', () => {
    expect(shouldDismiss(-DISMISS_AT)).toBe(true);
    expect(shouldDismiss(-DISMISS_AT + 1)).toBe(false);
    // A rightward drag has no meaning on this screen and must never remove anything.
    expect(shouldDismiss(60)).toBe(false);
  });

  it('claims a sideways drag and refuses a scroll', () => {
    // 40 px across, 4 px down: a swipe.
    expect(isHorizontalSwipe(40, 4)).toBe(true);
    // 40 px across, 60 px down: the customer is scrolling the feed, not dismissing anything.
    expect(isHorizontalSwipe(40, 60)).toBe(false);
    // A tiny jitter on a tap is not a gesture at all.
    expect(isHorizontalSwipe(6, 0)).toBe(false);
  });
});
