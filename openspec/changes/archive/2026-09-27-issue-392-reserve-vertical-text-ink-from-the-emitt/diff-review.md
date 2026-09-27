# Diff review

AUTHORS: agy, claude
REVIEWER: codex
VERDICT: APPROVE
ROUNDS: 2
TREE_SHA256: 19d77fbd3879d2ece5af2035fbb1ddba620b9902b86c8bb2f7249d7aadd14648
SPECS_SHA256: d2fa9d15c1ea42af6c61355c57851eccb48ed1c0144c9df36373882dde32a8cc

The complete implementation was reviewed against the approved layout-sizing delta. The first
round found that shaping each emitted line independently lost Typst's paragraph-wide script
context, causing `0\u{0301}\nα` to reserve about 3.84 pt of ink that Typst does not draw.

The fix shapes and segments the emitted block as one paragraph before slicing its runs into lines.
The permanent frame-calibration regression now compares several multiline mixed-script blocks
against Typst, and the reported 40 pt case renders and remains contained. The full Rust test suite,
format check, Clippy check, and strict OpenSpec validation pass on the fixed tree.
