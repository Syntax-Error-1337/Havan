import { useEffect, useState } from 'react';

/**
 * A value that settles before it is used.
 *
 * Typing into a search box that asks the server on every keystroke sends seven requests for "bedside" and shows
 * the answer to "b" first. This holds the value still for `ms` after the last change; the caller renders from the
 * settled value and the request only ever sees a word somebody finished typing.
 */
export function useDebounced<T>(value: T, ms = 250): T {
  const [settled, setSettled] = useState(value);

  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);

  return settled;
}
