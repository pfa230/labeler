## Context

See `proposal.md` — Why for the motivation and the measurements. What shapes the approach here is
where the two accent tokens came from and what already reads them.

`--accent-deep` `#b8420f` / `#f0784f` exists because ADR-0066 needed a legible `single` badge and
declined to re-tint `--accent`, which it lists as "the primary button background, the selected-card
border, the favorite star and the SVAR grid's hover and selection rows", deferring that to this issue.
Its light value is the same `#b8420f` this change adopts as `--accent`, so the colour is not new: #201
already put it on screen and judged it acceptable as accent-coloured text.

`ui/src/lib/contrast.ts` and `ui/src/theme.test.ts` also arrive from #201. The test parses
`theme.css` itself, so assertions read the shipped values rather than a copy. It covers only the badge
palette today: foregrounds `accent-deep` and `info` against backgrounds `accent-soft`, `info-soft`,
`surface` and `paper`. The plain `--accent` and the undefined `--accent-ink` are outside its reach,
which is why both failures survived a change that added a contrast test.

Two consumers depend on the token *names*, not just their values. `FormatBadge.tsx` names
`--accent-deep` as the `single` foreground. `setupTests.ts`'s `noBadgeStyling` guard rejects an element
whose inline style names `--info`, `--info-soft`, `--accent-deep` or `--accent-soft`, and three tests
(`Catalog`, `TemplateDetail`, `PreviewPane`) assert with it that a prose mention of a format has not
acquired the badge's colour treatment.

## Goals / Non-Goals

**Goals:**

- One accent per palette that clears 4.5:1 as text over every ground it is painted on.
- A defined `--accent-ink` in both palettes, with no call site free to supply its own.
- A theme test that mechanically covers every ratio the capability states, and that names the parts of
  the capability it cannot reach rather than implying it covers them.
- Screenshot evidence in both themes, because the change is visual and every accent surface moves.

**Non-Goals:**

- Re-tinting `--accent-soft`, `--info`, `--info-soft`, `--good`, `--bad` or any neutral. `--accent-soft`
  stays `#fbe9e2` / `#3a241c`: it clears its ratios against the new accent and touching it would widen
  the screenshot surface for no measured defect.
- Auditing every non-accent pairing in the palette. `--muted` on `--accent-soft` is 4.57 light and 6.86
  dark and passes, but a full palette audit is a different piece of work with a different acceptance.
- AAA. The target is AA, 4.5:1 for text and 3:1 for non-text components.
- Any change to focus rings, which are Tailwind `ring-2` utilities and carry no accent token.

## Decisions

### Darken `--accent` rather than repoint the ten text call sites

**Chosen:** light `--accent` `#e4572e` → `#b8420f`; dark unchanged at `#f0784f`.

| pairing (light)                | before | after |
| ------------------------------ | -----: | ----: |
| accent text on `--accent-soft` |   3.13 |  4.67 |
| accent text on `--surface`     |   3.68 |  5.49 |
| accent text on `--paper`       |   3.47 |  5.17 |
| `--accent-ink` on `--accent`   |   3.68 |  5.49 |

Dark needs no accent move: 5.18 on `--accent-soft`, 6.07 on `--surface`, 6.58 on `--paper`.

**Alternative — promote `--accent-deep` to the text token, leave `--accent` as a fill, and darken the
button labels.** This is viable and it is cheaper. Ten text locations repoint, no fill, border, star or
stripe re-tints, and the vermilion is never touched. It reaches the nineteen buttons too: white cannot
work on `#e4572e` (3.68), but a dark ink can, and `#16140f` on `#e4572e` is **5.00**. The existing
light `--ink` `#1c1a17` would also pass at 4.71.

Rejected on what it leaves behind rather than on what it cannot do:

- It turns every primary button in light mode from white-on-orange to near-black-on-orange. That is
  not a smaller visible change than deepening the fill, it is a different one, and it is the odder
  of the two: a primary action reading as dark text on a bright fill looks disabled or secondary
  next to the same button in dark mode, which keeps its light-on-dark reading.
- It makes the two-orange split permanent. ADR-0066 recorded that split as an accepted trade-off
  precisely because it was temporary, naming #210 as where it would be revisited
  (`docs/adr/0066-format-badge-carries-icon-colour-and-count.md:52-56`). Taking this option is
  choosing to keep it, and it should be recorded as that rather than as a saving.

The cost of the chosen option is one deliberate brand shift, screenshotted once. The cost of this one
is a permanently forked accent and an unusual primary button, carried indefinitely.

**Alternative — darken `--accent-soft` instead.** Rejected: reaching 4.5 from 3.13 needs the tint about
1.4× darker, at which point it stops reading as a soft tint and re-tints the selected card, the active
nav item and the two accent-tinted chips anyway, and it does nothing for accent text on `--surface`
(3.68) or `--paper` (3.47), which are the same defect on a different ground.

**Alternative — a milder darkening.** `#c9491f` reaches only 4.01 on `--accent-soft`. There is no value
that both preserves the current vermilion and passes; the cost is unavoidable, so it is paid once.

### Retire `--accent-deep` rather than keep it as a second name

With `--accent` at `#b8420f` light and `#f0784f` dark, `--accent-deep` holds the identical value in
both palettes. It is deleted, and `FormatBadge`'s `single` foreground becomes `var(--accent)`.

**Alternative — keep it as `--accent-deep: var(--accent)`.** Rejected: `theme.test.ts` parses literal
declarations out of the stylesheet, so a `var()` value reaches `contrastRatio` as a non-hex string and
throws by design. Keeping it as a duplicated literal is worse: two hexes that must be edited together,
with nothing enforcing it.

`template-format-badge` does not constrain this. Its requirements are written over resolved colours,
never token names: the two badges still resolve to different text colours and different fills, and its
"a sheet is not coloured as the accent" clause binds `--info`, which is untouched.

### `noBadgeStyling` widens to `--accent`

Deleting `--accent-deep` leaves that clause of the guard's regex matching nothing, silently weakening
three tests rather than failing them. The clause becomes `--accent`, which by word boundary also covers
`--accent-soft` and `--accent-ink`.

Verified this does not turn the three guarded assertions red: the catalog's format word is
`color: var(--ink)` (`Catalog.tsx:56`), the detail page's `Dimensions` row carries no inline colour
(`TemplateDetail.tsx:272`), and the preview pane's fallback link carries no inline style at all
(`PreviewPane.tsx:31`). None names an accent token.

### `--accent-ink` is a literal hex per palette, not a reference

`#ffffff` in light (5.49 on `#b8420f`), `#16140f` in dark (6.58 on `#f0784f`).

The dark value equals the dark `--paper`. It is written as its own literal rather than
`var(--paper)` for the same reason as above: the theme test reads literal declarations, and a
reference would make the pairing unassertable — the one place this token has ever gone wrong.

**Alternative — white in both palettes.** That is today's behaviour by fallback and is exactly the
2.80:1 defect. The light accent is dark enough to carry white; the dark accent is not, and the
requirement is the ratio, not a shared value.

**Alternative — a dark ink in both palettes.** Not available. Once `--accent` is `#b8420f` no dark
ink reaches AA on it: `#16140f` is **3.35** and even pure black is only **3.82**, because a deep rust
and a near-black are both dark. The light palette's ink must therefore be a light one, and the two
palettes take opposite inks by necessity rather than by preference.

White is not the only light ink that would pass: `--paper` `#faf8f3` reaches 5.17 and `--ink`'s dark
counterpart `#f2efe7` reaches 4.78. White is chosen over both because it is the highest at 5.49, and
because a primary button's label is the one place the UI should not tint away from pure white for no
reason. The constraint is *light*, and the choice within it is ours.

The two fills constrain the ink in opposite directions, and neither leaves both ends open. On
`#e4572e` only a **dark** ink passes: white is 3.68 and `--paper` `#faf8f3` is 3.47, while `#16140f`
is 5.00 and black is 5.70. On `#b8420f` only a **light** ink passes, as above. So darkening the accent
does not narrow a choice that existed, it swaps which end of the range the light palette must reach
for, and it swaps it to the end that matches the dark palette's convention of a light fill under a
dark ink being the odd case rather than the norm.

**Alternative — leave the fallback in place and only define the token.** Rejected: a call site naming
`#fff` keeps the option of diverging from the palette open, and the theme test cannot see a colour that
lives at nineteen call sites. All nineteen become `var(--accent-ink)`.

### The theme test replaces its `accent-deep` references, and states what it cannot reach

Three references are **replaced**, not added to. `REQUIRED` (`theme.test.ts:36`) swaps `accent-deep`
for `accent-ink`; `FOREGROUNDS` (`:42`) swaps it for `accent`; the resolved-colour comparison (`:57`)
compares `accent` against `info`. Leaving any of the three in place while the token is deleted turns
the suite red on a missing token, which is the loud failure `contrast.ts` was built for and is the
right failure, but it is not the intended end state.

Three assertions are added:

- `--accent-ink` over `--accent` at 4.5:1, in both palettes.
- the accent over `--surface` at 3:1, the non-text-component ratio for the selected template card's
  border.
- `accent-deep` absent from both palettes. This is a regression guard against this token in
  particular coming back, not enforcement of the one-accent requirement in general: a palette could
  satisfy it and still introduce a second accent shade under another name. Enforcing the general rule
  mechanically would mean deciding which of two hexes counts as "a shade of the accent", which is a
  judgement, not a computation, so the general rule is held by review and by the ADR.

Three badge tests assert the old token by name and are repointed to `var(--accent)` in the same step:
`FormatBadge.test.tsx:80`, `Templates.test.tsx:147,149`, `TemplateDetail.test.tsx:245,247`.

Three things in `specs/ui-colour-palette/spec.md` this test cannot reach, verified elsewhere rather
than claimed here. That every accent-filled control resolves its label from the palette is a call-site
fact, verified by `rg 'accent-ink' ui/src` returning no literal fallback. That a selection survives a
tint the eye cannot separate from the surface is a component fact, verified by the selected card's
border in the DOM and by the screenshots. And "no second shade of the accent an author could reach
for" is the judgement above, held by review rather than by assertion.

### The grid's accent surfaces are the focus outline and the resize grip, not a selection stripe

ADR-0066 lists "the SVAR grid's hover and selection rows" among the surfaces `--accent` paints, and
this change's first draft repeated it. It is not true of the shipped app. Four `--wx-*` entries in
`theme.css` name an accent token and none of them can fire:

- `--wx-table-select-background` and `--wx-table-select-border` style `.wx-selected`, which the grid
  sets only when its own row selection is enabled. `ConnectorBrowser.tsx:659` passes `select={false}`;
  selection in this browser is our own checkbox column, which paints no accent.
- `--wx-table-drag-over-background` styles `.wx-inactive`.
- `--wx-background-hover` styles only `.wx-icon.wxi-close:hover`, and no column the browser configures
  mounts that icon. The imported stylesheet has no `.wx-row:hover` rule at all, so rows never tint on
  hover.

Verified against the rendered DOM, not inferred: with the grid populated, the column picker open, a
filter typed, and rows clicked and ctrl-clicked, `.wx-selected`, `.wx-inactive` and `.wxi-close` all
count zero. A driven `Input.dispatchMouseEvent` hover leaves the row at `--surface`.

The grid is **not** free of the accent, though. `--wx-color-primary: var(--accent)` (`theme.css:43`)
is live, and the vendor paints two things with it: the focused cell's 1px outline
(`.wx-cell:focus{outline:1px solid var(--wx-color-primary)}`) and the column-resize grip. Both are
non-text marks, and they sit over two different grounds, so neither is covered by one assertion alone:

- a focused **body** cell is over `--surface`, which the 3:1 non-text assertion covers directly
  (5.49 light, 6.07 dark);
- a focused **header** cell and the resize grip are over `--paper`, because `theme.css:63` maps
  `--wx-table-header-background` to it. There is no separate 3:1 assertion for that ground, and none
  is added: the accent is already asserted against `--paper` at **4.5:1** as a text foreground
  (5.17 light, 6.58 dark), which subsumes the 3:1 a non-text mark needs.

Either way they deepen with the accent like everything else.

**The mapping is kept, not deleted.** `theme.css` maps the vendor's token set completely, forty-one
variables against the vendor's forty-one, and several non-accent entries are inert for the same
reason; deleting only the accent four would be inconsistent, and would mean that enabling a vendor
feature later silently inherits the vendor's greys instead of our palette. What is removed is the
*claim*: the comment above the block now records which entries cannot fire and why, and nothing in
this change cites a selection stripe or a row hover as an accent surface.

The selected template card's border is the non-text accent mark the 3:1 requirement is verified
against directly.

### ADR

**ADR-0071**, "One accent colour, dark enough to carry text, with a defined ink on its fill". It
partially supersedes **ADR-0066**: that record's `--accent-deep` token decision is replaced, while its
icon, count and border decisions stand. ADR-0066's README row gains
`(the --accent-deep token decision superseded by [0071])` and ADR-0071's row is appended.

The number was re-checked after the apply stage and **0070 was already taken**: `.worktrees/issue-197`
holds an untracked `docs/adr/0070-connection-connector-is-immutable.md` and its own README row, from
the same base commit. This change takes **0071**, which no worktree and no branch holds. `0067` is an
unused gap that stays unused. Re-check once more immediately before the commit: #200 and #212 are
still in flight with no ADR written.

## Risks / Trade-offs

- **The brand orange visibly moves.** `#b8420f` is a perceptibly deeper rust than `#e4572e`, on every
  button, chip, star and selected-card border in the app. → Accepted deliberately, not
  mitigated: no value both keeps the vermilion and passes AA. It stays in the same hue family and sits
  against the warm cream `--paper` the palette is already built on, and #201 has already shown it on
  screen as badge text.
- **A wide screenshot surface, so a missed surface is plausible.** → The accent surfaces are
  enumerable: 36 exact `var(--accent)` references over 16 runtime `.tsx` files, of which ten paint
  text (listed in `proposal.md` — Why) and nineteen paint a label on the fill. Six screens cover every
  one of the ten text sites and six of the nineteen fills, each in both themes: the **template grid** with a card selected and a card favourited (`Templates.tsx:164`,
  the selected-card border, the group chips, the New button); the **template detail** page
  (`TemplateDetail.tsx:79,300`); the **catalog** (`Catalog.tsx:42,186`); **Connect** with a resource
  selected (`ConnectorBrowser.tsx:456,598`, the Print button, and the grid's focused-cell outline; it
  has no selection stripe, see the decision below); a
  **settings** section (`PrintersSection.tsx:220`); and **Login** (`Login.tsx:62`). `Shell.tsx:23,69`
  ride along on all six. `Shell.tsx:134` is the mobile header and needs one narrow-viewport shot,
  since it is `md:hidden`.

  The thirteen fills not shot (`Setup.tsx:62`, `Import.tsx:559`, `NewTemplate.tsx:67`,
  `PrintForm.tsx:234`, `EmptyTemplates.tsx:28` and the rest) are deliberately out of the shot list,
  not overlooked: every one is the same `background: var(--accent); color: var(--accent-ink)` pair as
  the six that are shot, with no other token involved, so they cannot differ from them. Say so rather
  than implying the six screens reach all nineteen.
- **Two tests depend on token names and fail in different ways.** `theme.test.ts` fails loudly if
  `accent-deep` is removed while `REQUIRED` still names it, which is the good failure. `noBadgeStyling`
  fails silently, matching nothing and asserting nothing. → The task list changes both in the same step
  as the token deletion, and the widened guard is verified against the three prose sites named above.
- **The dark `--accent-ink` duplicates the dark `--paper` literal.** A later edit to `--paper` will not
  carry. → Acceptable: they are independent decisions that happen to coincide, and the theme test
  asserts the ratio that matters against `--accent`, not the coincidence.
- **Sibling worktrees may take an ADR number first.** This happened: issue-197 took 0070 from the same
  base. → Re-checked after apply and renumbered to 0071; re-check again before the commit, because
  checking `main` alone does not see an untracked ADR in a sibling worktree.

## Migration Plan

None. No persisted state, no config, no API surface. The change is a stylesheet edit plus call-site
edits, shipped in one commit; rollback is reverting it.
