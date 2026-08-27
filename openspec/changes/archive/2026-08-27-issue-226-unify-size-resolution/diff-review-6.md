codex
- MODERATE — The stale image matters. `docs/AUTHORING.md:16-18` promises every shown render is the shipped template’s actual output, while `docs/AUTHORING.md:344` embeds a 362×128 PNG—matching the old 51.1mm width at 180 DPI. Current output is 50.7857mm (`openspec/changes/issue-226-unify-size-resolution/tasks.md:85`), which rasterizes to 360px. This directly contradicts the adjacent worked result at `docs/AUTHORING.md:346-351`; regenerate the image before landing.

- The empty-node difference is genuinely sanctioned, not merely invisible: the contract makes the layout the rendered output (`specs/layout-sizing/spec.md:617-621`), keeps only the first line for non-multiline text (`:628-631`), and drops blank edge lines at emission (`:638-646`). The transcript confirms exactly 12 affected cases and 18 pre-existing bodiless measured-path alignments.

- The reported 47th difference is harness noise: both `homebox-qr | blank_both` bodies contain the identical `MissingField` result and no Typst; only the captured test summary differs. The corrected 29.6857mm content extent and 50.7857mm label width are also confirmed, with no other numeric-claim discrepancy found.

VERDICT: REVISE
