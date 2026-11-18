# 12 — The visual revamp: material, art fields, and a gauge that means something

Status: **approved direction, in build** · Owner: design + app · Last revised: iteration 57

This is the plan for making the Haven app look like something a customer would show a friend, without turning it
into a template and without lying about what it can do. It is written to be read before the code, and to be checked
against the code afterwards.

---

## 1. What we are fixing

The app is not ugly. It scores 7–9/10 per screen under review. It is **flat**:

- **One material for everything.** Every card is the same translucent white (or the same 6 % white in dark). Content
  and controls share a surface, so nothing floats and nothing recedes.
- **Category colour is a chip, not a field.** A bulb and a camera differ by a 40 pt circle and a border tint. At a
  glance, a grid of eight devices is eight grey rectangles.
- **Numerals are small.** The number is why the app exists — 21.5 °C, 58 %, 355 W — and it is drawn at body size in a
  card whose biggest element is the device name.
- **No gauge.** A device with a range (a thermostat, a dimmer, a cover) is shown as a slider, which tells you where
  the thumb is but not where the value sits in its range.
- **Nothing is ambient.** The screen says "6 offline" in a sentence. It does not *look* like a home that is half
  asleep.

## 2. The reference language, and what we take from it

Two references were supplied: a dark cinematic dashboard (photo hero, glass cards, radial thermostat, bar chart,
room capsule) and a lavender-glass tablet dashboard (photo tiles with gradient footers, big gauge, floating room
switcher).

What makes them feel premium: **photographic art direction**, **one bespoke data moment** (the gauge), **one accent
doing only state**, and **material restraint** — soft radii, hairline highlights, one shadow depth.

What we take:

| Taken | Why it works |
| --- | --- |
| **Device art fields** — a tinted, bloomed field with an oversized glyph behind each device | This is the honest stand-in for product photography. A white-label platform cannot ship photos of devices it has never seen; it *can* make every device look like a product shot in its own colour. |
| **Oversized numerals**, light weight, tabular | The reading is the point of the card. This is also Apple's own habit (`typography.md › Using system fonts`). |
| **A radial gauge for ranged values** | Answers "where am I in the range" at a glance, answers it with a shape rather than a sentence. |
| **Floating glass capsule for the room switcher** | Rooms are Haven's real hierarchy, and a capsule over scrolling content is the one place glass is *correct* (see §3). |
| **Semantic-only colour** | Colour already means category and state in Haven. Nothing decorative is added. |

What we **refuse**, and why:

- **Photographs of rooms and products.** We do not have them for arbitrary vendor hardware. Faking them with stock
  imagery is the exact "template with no point of view" failure the design lens warns about, and it would be a lie
  about a device the customer owns.
- **Glass on every card.** See §3 — it is a HIG defect, and it is also why those references feel soft-focus.
- **A percentage where a temperature belongs** (one reference reads `21.5%` for a thermostat) and **misspelled
  labels** (`Lightning`). Copy-paste surface, not design.

## 3. Layer discipline — the constraint that shapes everything

Apple is unambiguous, and it is the one rule that separates a designed app from a dribbble shot:

> `materials.md › Liquid Glass`: "**Don't use Liquid Glass in the content layer.** … including it in the content
> layer can result in unnecessary complexity and a confusing visual hierarchy."

> `liquid-glass.md › Review checklist` #1: "Glass appears only on floating controls and navigation. Glass on app
> backgrounds, cards, list rows, or content containers is a defect."

Haven today puts `color.glass` on **every card**. Under the rule above, that is the defect. So:

```
┌──────────────────────────────────────────────────────────────────┐
│ FUNCTIONAL LAYER — glass is correct here                          │
│  · tab bar            · sheets and the confirm dialog             │
│  · room capsule (floats over the scrolling grid)                  │
│  · a slider or toggle *while it is being dragged*                 │
├──────────────────────────────────────────────────────────────────┤
│ CONTENT LAYER — standard materials, never glass                   │
│  · device art fields (tinted, opaque, bloom)                      │
│  · tiles, hero, chart card, list rows                             │
│  · app canvas: the mesh                                          │
└──────────────────────────────────────────────────────────────────┘
```

Glass is not banned — it is **rationed** to three places, where it earns its keep because something colourful scrolls
beneath it. The tab bar already floats over content; the room capsule will; the confirm sheet already does.

The visible consequence: cards stop being translucent and become **tonal** — a near-opaque surface with a 1 pt inner
highlight on the top edge, a hairline border, and one soft shadow. That reads deeper than glass, not shallower,
because the surface has a light source.

## 4. Token plan

### 4.1 Colour (roles, not names)

Existing palette is kept; five roles are added. Numbers are against the surface they are actually drawn on.

| Role | Light | Dark | Use | Contrast |
| --- | --- | --- | --- | --- |
| `card` | `#FFFFFF` at 96 % | `#141821` at 88 % | content surfaces | text 17:1 / 15:1 |
| `cardEdge` | `rgba(255,255,255,0.9)` top hairline | `rgba(255,255,255,0.10)` | the light source on a card's top edge | — |
| `scrim` | `rgba(8,10,18,0.38)` | `rgba(0,0,0,0.42)` | behind text on an art field | white on it 8.6:1 |
| `track` | `rgba(120,120,128,0.16)` | `rgba(120,120,128,0.32)` | gauge track, slider rail | — |
| `fieldTint` | category accent at 10 % | at 22 % | the art field's base wash | glyph 4.6:1 min |

Category accents stay as they are — they already pass `make contrast` in both appearances, and they are the one thing
that makes a vendor's product look designed (§ `tokens.ts` comment).

### 4.2 Type

`typography.md › Using system fonts` — one family, hierarchy from size and weight. Two styles are added:

| Style | Size / line | Weight | Use |
| --- | --- | --- | --- |
| `numeral` | 34 / 38 | 300 (light) | the reading inside a tile |
| `gauge` | 44 / 46 | 200 (ultra-light) | the reading inside a hero gauge |
| `display` | 34 / 41 | 700 | screen titles (unchanged) |

Tabular figures (`fontVariant: ['tabular-nums']`) wherever a number can change, so a live reading does not shimmy.
Body stays 17, caption 13, tiny 11 — the accessibility floor is unchanged.

### 4.3 Shape and depth

| Token | Was | Now | Why |
| --- | --- | --- | --- |
| `radius.card` | 16 | **22** | a 22 pt radius on a 168 pt tile reads as a soft product, not a web card |
| `radius.field` | — | 12 | the art field inside a card |
| `radius.pill` | 999 | 999 | unchanged |
| `elevation.1` | none | `0 1 2 rgba(15,23,42,0.06)` | resting card |
| `elevation.2` | none | `0 8 24 rgba(15,23,42,0.10)` | the hero |
| `elevation.3` | `shadow` | `0 12 32 rgba(15,23,42,0.18)` | the floating capsule |

React Native renders shadows per platform; the shadow is drawn as a real `shadowColor/Offset/Opacity/Radius` on iOS,
`elevation` on Android, and `boxShadow` on web, from one helper so the three never drift.

### 4.4 Motion

Three moves, no more (`motion.md`: purposeful, brief, cancellable):

1. **Gauge sweep** — the arc animates from its old value to the new over 380 ms on a data change. It is the one
   orchestrated moment, and it is the product's signature.
2. **Press** — 0.97 scale, 140 ms, on every card (already exists on some).
3. **Capsule** — the room pill slides under the active word, 220 ms.

Reduced motion turns 1 and 3 off; 2 stays (it is feedback, not decoration).

## 5. The signature element: the art field

Every device gets a **field** instead of a white hole:

```
┌──────────────────────────────┐   base:   category accent at 10 % (light) / 22 % (dark)
│ ◜ bloom                      │   bloom:  a radial gradient of the same accent, 40 % → 0,
│      ⌾                       │           one corner, radius = 1.1 × card width
│                    (glyph)   │   glyph:  the category icon at 96 pt, accent at 12 %,
│                              │           anchored bottom-right, half off the edge
│  21.5 °C                     │   edge:   1 pt inner top highlight (cardEdge)
│  Living Room · Silent 4h     │   scrim:  only under text that sits on the field
└──────────────────────────────┘
```

Three rules:

- **One bloom per card, one glyph, one accent.** Not a mesh, not two gradients. The restraint is the design.
- **The glyph is decorative and hidden from screen readers** (`accessibilityElementsHidden`), because the device's
  name and reading are already announced.
- **Text on the field gets the scrim**, never a drop shadow — a shadow on type is a 2010 tell, and it fails at light
  text on a light field.

This is what makes a grid of eight devices read as eight *things* rather than eight cards, and it is why the
photographic references work: replace photograph with generated field, keep everything else.

## 6. Screens

### Compact (390 × 844)

```
┌─────────────────────────────────────┐
│ ◦ Home        ⌂ Workshop ▾     ⌕ ⌁ │  header: home switcher, search, alerts
│ Some devices are not reporting      │  ambient banner (only when true)
│ ┌─────────────────────────────────┐ │
│ │  ╭────────╮              18.3   │ │  HERO: gauge card, elevation 2
│ │  │ gauge  │            °C       │ │   · gauge = value in its min/max
│ │  ╰────────╯  Hallway Sensor     │ │   · art field behind, glyph bottom-left
│ │   Living Room · last reading 2h │ │
│ └─────────────────────────────────┘ │
│  All  Living  Kitchen  Hall   ───▶  │  ROOM CAPSULE: glass, floats, sticky
│ ┌───────────┐ ┌───────────┐         │
│ │ art field │ │ art field │         │  tiles: radius 22, elevation 1
│ │  ⌾        │ │  ⌾        │         │
│ │ 58 %      │ │ On        │         │
│ │ Humidity  │ │ Lamp      │         │
│ └───────────┘ └───────────┘         │
└─────────────────────────────────────┘
        ▔▔▔▔ tab bar (glass) ▔▔▔▔
```

### Wide (≥ 768, web and tablet)

The same components, two-up: hero spans 2 columns, tiles 3 per row, the room capsule moves to the header's
second line. No new layout code — the grid goes from 2 to 3 columns by `minWidth`, because `layout.md` asks for
layout that follows available space rather than device.

## 7. Component specs

| Component | File | Spec |
| --- | --- | --- |
| `DeviceArtField` | `shared/ui/DeviceArtField.tsx` | `(accent, glyph, height, rounded)` → base wash + radial bloom + 96 pt glyph at 12 %; decorative only |
| `Card` | `shared/ui/Card.tsx` | surface `card`, radius 22, hairline edge, inner top highlight, `elevation` 1–3, optional press |
| `Gauge` | `shared/ui/Gauge.tsx` | SVG ring, `strokeWidth` 10, track `track`, arc = accent, 270° sweep starting at 135°; centre slot |
| `DeviceTile` v2 | `entities/device/ui/DeviceTile.tsx` | art field top 96 pt, numeral 34, name 15, status 11; toggle inside the field's top-right |
| `HeroCard` v2 | `widgets/home/HeroCard.tsx` | gauge when the point has min **and** max, sparkline otherwise; floating status chip |
| `RoomCapsule` | `widgets/home/RoomCapsule.tsx` | glass capsule, horizontal scroll, active pill slides, `role="tab"` |
| `TabBar` | `shared/ui/TabBar.tsx` | keep; it is already the functional layer. Radius to 34, shadow to elevation 3 |

## 8. Waves and gates

Every wave ends the same way: **build → screenshot at 390 × 844 in both appearances → score with the design lens out
of 10 → fix the worst finding → `make check` + app tests + e2e → commit**. No wave is "done" on code review alone.

| Wave | Scope | Done when |
| --- | --- | --- |
| **W0** | Tokens: `card`, `cardEdge`, `scrim`, `track`, `fieldTint`; radii; elevations; numerals; motion | `make tokens` + `make contrast` green, nothing regresses, screenshots unchanged-but-consistent |
| **W1** | `DeviceArtField`, `Card`, `DeviceTile` v2 | a dashboard grid where eight devices look like eight things; a11y gate green; tile score ≥ 8.5 |
| **W2** | `Gauge` + `HeroCard` v2 | gauge animates on a live change; hero score ≥ 9; chart honesty rules preserved (gaps, band, window) |
| **W3** | `RoomCapsule` + dashboard chrome + wide layout | rooms filter the grid; capsule is keyboard/screen-reader navigable; 3-column wide layout proven at 1024 |
| **W4** | Device detail (art header + gauge), alerts, scenes, automations, settings | every screen ≥ 8.5; no screen uses glass in the content layer |
| **W5** | Motion, reduced-motion, perf (frame budget), docs, `docs/06` update | 60 fps on the dashboard with 200 tiles, reduced motion verified, docs written |

## 9. What we are removing

The design directive says "make it stunning". The stronger move is what comes **out**:

- **The translucent card.** One material becomes two layers.
- **The inline summary sentence** ("6 devices · 4 rooms", "2 offline") in the header — it becomes a single ambient
  line that only appears when something is wrong.
- **The category filter chips** on the dashboard, replaced by the room capsule. Two switchers on one screen is one
  too many, and rooms are the hierarchy a person actually thinks in.
- **The sparkline inside the hero** when a gauge is possible. One data moment per card.

## 10. Risks and how they are handled

| Risk | Handling |
| --- | --- |
| Glass on web ≠ glass on iOS | The functional layer uses `backdrop-filter` on web and a layered translucent fill on native. No new dependency: the app's own `Glass` already does this, and `expo-mesh-gradient` is not supported on web, so it is out. |
| Art fields become noise with 200 devices | The field is a flat wash + one radial; it costs one gradient per card, measured against the existing criterion "the dashboard must stay at 60 fps". |
| Contrast on tinted fields | Every text-on-field pair goes through `make contrast`, which measures rendered pixels, not tokens. |
| White-label vendors | Everything is from tokens; a vendor's accent replaces the category accent's *base* hue but the field's alpha stays fixed. |
| Native cannot be verified here | Stated as a limit in every report: web at 390 × 844 is real browser layout, not a native build. |
