#!/usr/bin/env bash
set -euo pipefail

# Requires LABELER_API_TOKEN in the environment: all /api routes need auth (ADR-0017).
# Create a token in the UI (Settings) and export it before running this script.
HOST=${HOST:-http://localhost:8080}
OUT=${OUT:-avery-sheet.pdf}

# avery5163 is the starter sheet template: POST /api/batch in download mode lays the labels
# into slots and returns one paginated PDF. This script renders three filled slots.
#
# The multi-variant layout (with orientation and outline options) now lives in
# tests/fixtures/templates/avery5163_asset_tag.yaml.
#
# Templates are no longer seeded on first run (#137), so install it first if this 404s:
#   curl -fsSL https://raw.githubusercontent.com/pfa230/labeler/main/catalog/sheet/avery/avery5163.yaml \
#     | curl -fsS -X POST "$HOST/api/templates" \
#         -H "Authorization: Bearer ${LABELER_API_TOKEN:?}" \
#         -H 'content-type: text/yaml' --data-binary @-
curl -sS -X POST "$HOST/api/batch" \
  -H 'content-type: application/json' \
  -H "Authorization: Bearer ${LABELER_API_TOKEN:?set LABELER_API_TOKEN}" \
  -d '{
    "template":"avery5163",
    "mode":"download",
    "labels":[
      { "data": { "message": "BOX.073 — Floor Grinder" } },
      { "data": { "message": "BOX.074 — Angle grinder, dust shroud" } },
      { "data": { "message": "BOX.075 — Spare discs" } }
    ]
  }' > "$OUT"

echo "Wrote $OUT"
