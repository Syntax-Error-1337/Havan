import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';

import { TableFrame } from './TableFrame';

/**
 * jsdom has no layout engine, so these assertions cover what a test can honestly claim: the region
 * exists, it is named, and it is reachable from the keyboard. Whether the table actually scrolls at
 * 420px is a rendering fact and was measured in a browser, not here.
 */
describe('TableFrame', () => {
  const table = (
    <table>
      <thead>
        <tr>
          <th>Serial</th>
        </tr>
      </thead>
      <tbody>
        <tr>
          <td>SN-1</td>
        </tr>
      </tbody>
    </table>
  );

  it('exposes the scroller as a named region', () => {
    render(<TableFrame label="Fleet devices">{table}</TableFrame>);

    const region = screen.getByRole('region', { name: 'Fleet devices' });
    expect(region).toHaveClass('table-frame');
    expect(region).toContainElement(screen.getByRole('table'));
  });

  it('is focusable, so the table can be scrolled without a trackpad', () => {
    render(<TableFrame label="Fleet devices">{table}</TableFrame>);

    // tabIndex 0, not -1: a scroll container nobody can focus is a scroll container keyboard users
    // cannot scroll, and there is no visible scrollbar to hint that content continues.
    expect(screen.getByRole('region', { name: 'Fleet devices' })).toHaveAttribute('tabindex', '0');
  });

  it('keeps the table inside the frame rather than wrapping it in a div soup', () => {
    const { container } = render(<TableFrame label="Audit trail">{table}</TableFrame>);

    expect(container.querySelectorAll('table')).toHaveLength(1);
    expect(container.firstElementChild?.querySelector('table')).toBeTruthy();
  });
});
