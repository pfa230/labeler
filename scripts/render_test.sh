#!/usr/bin/env bash
set -euo pipefail

# Requires LABELER_API_TOKEN in the environment: all /api routes need auth (ADR-0017).
# Create a token in the UI (Settings) and export it before running this script.
HOST=${HOST:-http://localhost:8080}
OUT=${OUT:-test.pdf}

# "test" is a sheet template: POST /api/batch in download mode lays the labels into
# slots and returns one paginated PDF.
curl -sS -X POST "$HOST/api/batch" \
  -H 'content-type: application/json' \
  -H "Authorization: Bearer ${LABELER_API_TOKEN:?set LABELER_API_TOKEN}" \
  -d '{
    "template":"test",
    "mode":"download",
    "labels":[
      {
        "data": {
          "url": "https://example.com/BOX.073"
        }
      },
      {
        "data": {
          "url": "https://example.com/BOX.075"
        }
      }
    ]
  }' > "$OUT"

echo "Wrote $OUT"
