# The vendor portal, walked like a customer walks it

`docs/13` did this for the app. This is the same pass over the portal — the screen a vendor's own staff live in, and the
one that decides whether "one app, per-device licences" feels like a platform or like a spreadsheet with a login.

Method: sign in as the demo vendor, walk each screen at 1440 × 900 with Chrome DevTools Protocol at a real desktop size,
read the rendered text out of the DOM rather than trusting the source, and score the screenshot with a design pass before
touching anything. The evidence is the screenshot plus the API response behind it, so "the column is empty" can be told
apart from "the query did not return the field".

## Fleet — found

| What | Severity | Why it matters |
| --- | --- | --- |
| `200 of 1,909 devices shown` | **P1** — fixed | "Shown" was read as "on screen". Eleven rows are visible while two hundred are loaded, so the number contradicted what a person could see. It now says **loaded**, which is the fact. |
| `Load more` / `Load all 1,909` | **P1** — fixed | Neither said what it would fetch. They now read **Load 200 more** and **Load the other 1,709** — the second no longer looks like a total that disagrees with the first. |
| `STATE` over **All devices**, beside a column called **Status** | **P1** — fixed | Two vocabularies for one control. The label is **Status**, the empty option is **All statuses**, and the options are statuses rather than devices. |
| Status cell read as one phrase | **P1** — fixed | `offline` (a pill) and `bound` (plain muted text) ran together as "offline bound", which is not a status anybody has. Binding is now a quiet chip with its own shape, a `title` that says *Bound to a customer*, and *In stock: manufactured, never claimed* for the other case. |
| `—` in the Firmware column, all rows | P2 — fixed | It was honest: the demo serials have never reported firmware (`fw_version: null` in `/v1/vendor/devices`). The dash now carries a title saying so, because a column of dashes is otherwise missing data or a broken query. |

## Fleet — sorted (the iteration after the first pass)

Serial, Name and Last seen are now sortable, and **the cursor continues the order**. The keyset recomputes the ordering
value of the cursor row from its id, so a cursor stays a bare id and page two of "serial A–Z" is the page after it, not
the page after "newest first". The column is a closed enum mapped to a literal expression — an unknown one is a 422 that
names the value, because that string would otherwise end up in an `ORDER BY`. Defaults follow how a field is read (text
ascending, timestamps descending) and a second press turns it round. `aria-sort` on the header says which column is
ordered and which way, which is the only way a screen reader can know.

A device that has never reported sorts as older than every device that has: without `COALESCE(last_seen_at, 'epoch')`
the tuple comparison goes NULL and those rows disappear from page two onwards.

## Fleet — opened (the iteration after sorting)

**Connection and Binding are two columns.** They were one cell holding two unrelated facts — a device that is offline in
a customer's house is a support call, and one that is offline in stock is not. The table now has seven columns, and the
header row and every body cell share one grid (measured: each `th` and its `td` sit at the same `left`).

**A row opens.** The serial is the control — it is the thing a support engineer is holding, and a separate icon column
would be a second, smaller target for the same press. `aria-expanded` and `aria-controls` say what happened, one row is
open at a time, and the panel is a table row rather than a card: it spans all seven columns, carries a tinted background
and a hairline top, and reads as part of the table instead of a popover inside it.

It shows what the table has no room for — device id, product key, manufacture date, exact last-seen — and says out loud
that the customer is deliberately absent: *"Who owns this device is deliberately not shown. A vendor sells hardware; the
person who bought it is not their user."* A vendor who cannot find the owner should know that is a decision, not a field
they forgot to ask for.

**Last seen carries both readings.** The cell shows "1d ago" and its title holds the exact local timestamp, because the
duration answers *is it alive* and the timestamp answers *was it alive when the customer called*.

## Fleet — still open

- **Status and Firmware do not sort.** Status is now split in two, so the question got smaller; firmware is mostly absent
  across the demo fleet and has no defensible ordering yet.
- Nothing about paging. The controls now live in a bar **inside the table's own scroll container, stuck to its bottom**:
  count on the left, actions on the right. Measured at 1440 × 900 mid-list — the bar's bottom edge equals the frame's
  (527 px) while `scrollTop` is 9,759, so it is a footer in reach rather than a row above the fold. It is a plain block,
  not a `tfoot`: a table cell is laid out by the table's algorithm and `position: sticky` on it is a promise the browser
  does not keep — the first attempt silently did nothing at all.

## Audit — found and fixed

The trail answered "when" and "what action" and nothing else. A reviewer's first question is *who*, and the column said
**`user 01a1cdb7`** — a uuid prefix that cannot be resolved without leaving the screen.

| What | Why it mattered | Now |
| --- | --- | --- |
| `Who` was a uuid prefix | "Who did this" is the question an audit trail exists to answer, and the person reading it is usually answering it for somebody else | The actor's **email**, resolved at read time by joining `vendor_users`. A member since removed keeps the id — which is itself worth seeing in a trail |
| Timestamps to the minute | Four entries shared `06:35`, so "newest first" could not be checked against the screen | To the **second**, in local time |
| `Target` was `vendor_webhook 01a1eba6` | A kind and an id is a row in a table, not an answer | The **url / name / email** the action recorded, with the id in the title |
| `webhook.created` and `api_key.revoked` recorded no subject at all | The trail could not say *which* webhook or key; the detail was never written, so nothing downstream could recover it | Both record `url` and `name` when written. The key's name is read **before** the key is revoked, because a revoked key cannot be looked up |

Old rows keep their old detail. There is no honest way to backfill what was never recorded, and inventing one would be
worse than the gap.

Still open on this screen: no filter by action or actor, no search, and no export — an auditor with a specific question
still reads the whole feed.

## What was not found

The rest of the portal held up: Licences states the model in one sentence ("charged when a device is activated, not when
a serial is manufactured"), Support explains a bundle before offering it, and Products shows certification as its own
column rather than folding it into status. Those are the sentences that make a platform explainable to a vendor's finance
team, and they are the reason this document is short.
