# Design

## Context

The print form's Copies box is `<input type="number">` between the − and + buttons, and its `onChange` already clamps every value through `clampCopies` to [1, 100], turning anything non-numeric into 1. Browsers draw spin buttons only for `type="number"`.

## Goals / Non-Goals

**Goals:**
- The print form's Copies box shows no browser spin buttons.

**Non-Goals:**
- Connect and Import Copies boxes, and every other numeric field: none has custom buttons, so each shows one stepper already.

## Decisions

**Make the box `type="text"` with `inputMode="numeric"`, and drop its `min` and `max`.** A text box has no spin buttons in any browser, `inputMode="numeric"` keeps the number keypad on phones, and `clampCopies` already enforces the range the dropped attributes declared.

Hiding the spinner with CSS (`appearance: textfield` plus the `::-webkit-inner-spin-button` and `::-webkit-outer-spin-button` pseudo-elements) is equally small but untestable: jsdom renders no spin buttons, so no test can fail if those rules go missing or a browser stops honouring them. With a text box, the scenario's "not a spin button" is the input's accessible role, which a test reads directly.

## Risks / Trade-offs

- [The box loses ArrowUp/ArrowDown stepping and the mouse-wheel stepping a number input has on desktop] → The − and + buttons cover stepping; typing a number still works.
