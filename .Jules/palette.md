## 2025-02-28 - Focus States for Grid Action Buttons
**Learning:** Icon-only action buttons within data grids (like the react-data-grid implementation) often lack default focus indicators because they aren't standard form inputs. This makes them invisible to keyboard-only users navigating the grid actions column.
**Action:** Always verify keyboard focus states (`focus-visible:ring-2` and `focus-visible:outline-none`) on icon-only buttons (`⧉`, `✕`), particularly when they reside inside non-standard layout components like data grids.
