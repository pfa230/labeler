## REMOVED Requirements

### Requirement: Auto-width text measures to its content and renders into its alignment slot

**Reason**: The requirement exists to make `alignment.horizontal` decide an auto-width text item's
box: the fitted width when left-aligned, the alignment slot when centred or right-aligned. Under the
`layout-sizing` protocol an item's box never depends on its alignment. The author writes `fill` to
say "stretch to the frame" and `alignment.horizontal: center` to say "centre within it", which
produces the same rendered result for the case this requirement was written for while removing the
inference. The requirement also rests on `auto`, which the same change removes from the size
vocabulary, so it has no valid spelling left. ADR-0059 is superseded.

`auto-length-layout` holds no other requirement, so the capability is removed rather than modified.
Everything it covered now lives in `layout-sizing`: what an item contributes to a dynamic-width
label's width, what box it is drawn into, and how `max_w` bounds both.

**Migration**: Rewrite the item's width with the meaning it actually wanted.

| Was | Becomes |
| --- | --- |
| `size: [auto, h]`, `alignment.horizontal: left` (or omitted) | `size: [content, h]` |
| `size: [auto, h]`, `alignment.horizontal: center` or `right` | `size: [fill, h]`, alignment unchanged |

The second row is the expected migration for the text in each of the four bundled Brother tape
templates; each text's enclosing explicit-`auto` container becomes `fill`, which reports its children
upward and takes the resolved frame downward. Those edits are carried out by this change and receive
before/after visual acceptance, because rewriting a template is visual work that ends at an inspected
render rather than at a parse. A template left on `auto` fails validation with a message naming both
replacements and is quarantined per ADR-0058, so no template silently relayouts.

#### Scenario: The removed inference is not applied to an old spelling

- **WHEN** a template leaves the removed `size: [auto, h]` spelling on an auto-width text
- **THEN** the template is quarantined with a message naming `content` and `fill`
- **AND** the renderer never infers a box from that text's horizontal alignment
