/**
 * A table that stays readable when the space is narrow.
 *
 * The wrapper is the scroll container, not the table: a `<table>` cannot be scrolled without
 * changing its display type, which drops the row/column semantics that make it a table at all.
 *
 * `role="region"` plus a name is what turns "some overflow" into a landmark a screen reader can
 * find, and `tabIndex={0}` is required because a scroll container that only responds to a
 * trackpad is unusable by anyone driving the page from the keyboard.
 */
export function TableFrame({
  label,
  children,
}: {
  /** Names the region for assistive technology; say what the table lists, not "table". */
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="table-frame" role="region" aria-label={label} tabIndex={0}>
      {children}
    </div>
  );
}
