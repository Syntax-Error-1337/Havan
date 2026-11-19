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

### Narrowing it (the iteration after)

Two filters, because the two questions are "what happened" and "who did it": **Action** (exact — the values are a closed
set, and the field suggests the ones the trail actually contains rather than a list hardcoded in the page) and **Actor**
(part of an address, because the person asking usually has half a memory of it). Both are trimmed and capped in the
domain, and both travel with the cursor — page two of `api_key.revoked` is the second page of that action, not the second
page of everything.

An empty answer under a filter says so: *"Nothing matches the action api_key.revoked … this is an empty answer, not a
missing one."* The unfiltered empty state stays separate.

Found while testing it: the page **returned early while loading**, which unmounted the filter box the moment it was typed
into — every keystroke ended in "Reading the trail…" and a lost caret. The header and filters now stay put; only the list
reports that it is reading.

### The file (and a note on how *not* to do it)

**Export CSV** asks the server for `GET /v1/vendor/audit.csv`, with the same filters, and hands the bytes to the browser.
It deliberately does **not** build a blob from the rows on screen: what is loaded is a page, and a file named "audit" that
silently holds fifty of four thousand rows is worse than no file at all — it is a document that looks like evidence.

Every field is quoted and inner quotes are doubled, which is not laziness: a detail blob carries JSON, and a comma or a
newline inside one is ordinary. A file that shifts a column because somebody's key name had a comma is a file that lies
about what happened — the test creates a key literally named `shipped, "v2"` and asserts all nine columns line up.

The scan stops at 10,000 rows and says so, in an `x-audit-truncated` header the portal turns into a sentence the operator
sees. A cap nobody is told about is the same lie in a different place.

Reuses `download()` from the API client rather than opening its own `fetch`: a second place that remembers the bearer
token is a second place to forget it.

## Team — found and fixed

Three defects on one screen, all about what a control promises.

| What | Why it mattered | Now |
| --- | --- | --- |
| **Remove** on every row, including your own and the last owner's | The server refuses both ("you cannot remove yourself; ask another owner"; a sole owner's departure leaves the account unowned), so two of those buttons could only ever fail | Your row reads **You**; a sole owner's row reads **Owner**. The others offer Remove |
| Remove with **no confirmation** | One press beside a colleague's name, and it cannot be undone | Asks first, naming the person and what they lose |
| **No way to withdraw an invitation** | An invitation is a working credential for seven days; one sent to the wrong address could only be waited out | `DELETE /vendor/team/invites/{id}` and a **Withdraw** on each pending row, with a confirmation. Owners and admins may withdraw — whoever may invite may un-invite. A second withdrawal is a 404, because "already gone" and "never existed" are the same answer to the caller |

The signed-in member is read from the token's own subject — unverified, because the server decides what anybody may do and
this only decides which buttons are worth drawing.

## API keys — found and fixed

| What | Why it mattered | Now |
| --- | --- | --- |
| The State column said **`unused`** | That is a *usage* fact sitting in the *state* column, beside a "Last used: never" cell saying the same thing — and it left the reader to work out whether an unused key still worked | State is **`active`** or **`revoked`**; "never used" belongs to the column that means it. Each badge carries a title saying what the state implies |
| Revoked keys were mixed through the list | Rotating a credential starts by finding the keys in use; history pushed the two actionable rows off the first screen | Active first, newest first inside each group |
| **Revoke** had no confirmation | It breaks whatever is using the key, immediately and for good | Asks first, naming the key, its prefix, and that it cannot be restored |
| A revoked row showed `—` under Actions | A dash in a column called Actions reads as "no actions, and no reason" | Nothing is drawn — the badge beside it already says why |

## Webhooks — found and fixed

| What | Why it mattered | Now |
| --- | --- | --- |
| **No delivery history** | An endpoint that has been refusing for a week looks exactly like one nobody has needed yet. A vendor had no way to tell whether their own ERP was receiving anything | A **Last delivery** column, from the newest `webhook_outbox` row per endpoint in the same query (`LATERAL`, so the list is not multiplied by history): *"delivered after 1 attempt"*, *"failed 3 attempts: 502 Bad Gateway"* (in the error colour), or *"nothing yet"*. The full timestamp is in the cell's title |
| **Remove with no confirmation** | Removing an endpoint silently stops deliveries a vendor's systems depend on | Asks first, naming the URL |

The sentence is built in the domain (`WebhookDelivery::describe`) as well as the portal, from the same three fields — the
two cannot drift about what "failed 3 times" means.

### One leftover, said plainly

Proving the join needed an outbox row that does not exist in the demo fleet, so one was inserted by hand and the page
verified against it. The delete that would have removed it again needed approval and was not granted, so the row is still
in the development database. `delete from webhook_outbox where last_error = '502 Bad Gateway'` takes it out; nothing
depends on it, and it is the only fabricated row this audit added.

## Rollouts — found and fixed

| What | Why it mattered | Now |
| --- | --- | --- |
| The column headed **Rollout** held a percentage | The header promised the name of a rollout and the cell held *how much of the fleet it covers* — the reader had to work out that 100% was a target, not a rollout | **Target**, with a title saying what the percentage means |
| **Roll back** had no confirmation | It changes firmware on every device the rollout reached | Asks first, naming the product, the share of the fleet, and that devices go back to the previous firmware |
| The **Firmware** link only appeared on finished rollouts | A draft is being written against an image and a running rollout is delivering one; the link was offered only at the moment there is nothing left to decide | Present in every state, beside the action |

The rest of the screen is unusually good: the progress cell leads with failures rather than a percentage ("3 failed",
"108 failed"), a halted campaign says *stopped*, not *done*, and a started rollout that matched nobody says so.

## Accessibility — measured, and two real defects

The app got three hand-written rules and 31 touch-target violations fixed; the portal had none, which is the worse half of
the omission — a vendor is an employee on a mandated display, using this all day. `portal/e2e/10-a11y.spec.ts` adds the
same three rules, without `axe` (MPL-2.0, ADR-002):

1. **every control has an accessible name** — passed everywhere, first run;
2. **body text meets WCAG AA against the colour it is actually painted on**, computed from rendered pixels, not from the
   palette: one failure, the **"certified" badge at 3.35:1** against its own 18 % tint. Green-on-green is the classic
   case a token check misses and a rendered check catches. The badge text is now mixed 55 % toward black; 4.9:1;
3. **every table header names its column** — **40 `<th>` elements across 15 pages had no `scope`**, which is what tells a
   screen reader which column a cell belongs to in a table read out of order.

A fourth rule came later, from the same family: **the tab said "Haven — Vendor Portal" on every screen** (WCAG 2.4.2,
page titled). Three open tabs were indistinguishable and a screen reader announced the same context after every
navigation. `app-shell/title.ts` maps the route to a name by longest prefix — so `/products/:id/firmware` stays under
Product, and `/admin/audit` does not read as the vendor's audit log — and the shell calls it once, for every screen
including the console. Proved by commenting the call out: the test failed with `/ is titled "Haven — Vendor Portal"`.

All 8 assertions pass now, and the portal suite is 92 with them.

## Products — the list needed nothing, the editor needed one thing

The list is honest about the rule it enforces ("3 products · a product must be published before a customer can pair
it"), certification is its own column rather than folded into status, and deleting asks.

The editor is a wall of raw JSON, and that is a deliberate choice rather than a defect: the thing model *is* JSON,
publishing is final, and the screen explains the two declarations the JSON does not advertise (`radio`,
`report_interval_secs`) better than most documentation would.

What it did get wrong: a **published** product still showed **Validate** and **Save draft**, both disabled and both
unexplained — two controls that can only ever fail, beside one that works. They are gone for a published model, replaced
by the sentence the server would have answered with: *"A published thing model cannot be edited — publishing is final,
because installed devices already read it. To change it, create a new product with a new key; the old one keeps working
for the devices that have it."*

The read-only textarea is genuinely `readOnly` — checked in the DOM, not by eye — so the JSON cannot be typed into and
lost.

## What was not found

The rest of the portal held up: Licences states the model in one sentence ("charged when a device is activated, not when
a serial is manufactured"), Support explains a bundle before offering it, and Products shows certification as its own
column rather than folding it into status. Those are the sentences that make a platform explainable to a vendor's finance
team, and they are the reason this document is short.
