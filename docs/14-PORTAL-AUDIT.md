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

## Fleet — still open

- **No sorting.** Serial, Status, Firmware and Last seen are all natural sort targets and none sort.
- **No row actions.** A fleet row cannot be opened; a support engineer reading a serial has to leave the screen.
- **Connectivity and binding in one column.** They are two dimensions — a device can be offline and unbound, which is a
  different problem from offline and in a customer's house. Separate columns, or one chip that says which question it
  answers, is the honest shape.
- **Last seen is a relative duration only** (`1d ago`). For a fleet table an exact timestamp belongs on hover.
- **Load controls are above the table by design** — deliberate, because at 1,900 devices the footer is two hundred rows
  away — but they scroll out of reach just the same. A sticky footer is the fix.

## What was not found

The rest of the portal held up: Licences states the model in one sentence ("charged when a device is activated, not when
a serial is manufactured"), Support explains a bundle before offering it, and Products shows certification as its own
column rather than folding it into status. Those are the sentences that make a platform explainable to a vendor's finance
team, and they are the reason this document is short.
