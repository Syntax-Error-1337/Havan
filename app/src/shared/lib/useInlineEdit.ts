import { useCallback, useState } from 'react';

/**
 * The behaviour behind "tap the name, change it, press Save".
 *
 * Two screens rename things — the settings list and the device header — and they look nothing alike:
 * one is a row with a value column, the other is a 34 pt title inside a hero card. What they share is
 * the part that is easy to get wrong: what the draft starts as, what counts as a change, and what
 * happens when the person empties the field.
 *
 * Rules, in one place:
 * - opening a draft always starts from the current value, never a half-typed leftover;
 * - whitespace is not a name, so a blank save is a cancel;
 * - saving the same value is not a request.
 */
export function useInlineEdit(value: string, onSave: (next: string) => void) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);

  const open = useCallback(() => {
    setDraft(value);
    setEditing(true);
  }, [value]);

  const cancel = useCallback(() => {
    setEditing(false);
    setDraft(value);
  }, [value]);

  const save = useCallback(() => {
    setEditing(false);
    const next = draft.trim();
    if (next.length > 0 && next !== value) onSave(next);
  }, [draft, value, onSave]);

  return { editing, draft, setDraft, open, cancel, save };
}
