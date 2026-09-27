TREE_SHA256: df4789a93a7b489f609c1f8d1c8b36d29cfc3bb05a470bb98d178c01f4b34731
SPECS_SHA256: d2fa9d15c1ea42af6c61355c57851eccb48ed1c0144c9df36373882dde32a8cc

[P2] [verified] **Independent line shaping reserves ink Typst does not draw** ([helpers.rs:1074](/home/pfa/projects/labeler/.worktrees/issue-392/src/render/helpers.rs:1074)). For top-aligned `0\u0301\nα` at 20 pt, weight 400 and default spacing, the helper raises the acute 393 font units higher than Typst’s combined Greek script run, calculating approximately 43.07 pt of required height instead of 39.23 pt and incorrectly rejecting a 40 pt box; this is verified through HarfBuzz probes and Typst’s collection, shaping and line-slicing code. Preserve the emitted block’s script context, add multiline Typst-frame calibration, and correct [design.md:72](/home/pfa/projects/labeler/.worktrees/issue-392/openspec/changes/issue-392-reserve-vertical-text-ink-from-the-emitt/design.md:72), which incorrectly claims bundled Inter cannot exhibit this discrepancy.

VERDICT: REVISE
