#!/usr/bin/env bash
set -euo pipefail

HOST=${HOST:-http://localhost:8080}
OUT=${OUT:-test.pdf}

# "test" is a sheet template: POST /batch in download mode lays the labels into
# slots and returns one paginated PDF.
curl -sS -X POST "$HOST/batch" \
  -H 'content-type: application/json' \
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
