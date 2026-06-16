#!/usr/bin/env bash
set -euo pipefail

HOST=${HOST:-http://localhost:8080}
OUT=${OUT:-avery-horizontal.pdf}

# avery5163 is a sheet template: POST /api/batch in download mode lays the labels into
# slots and returns one paginated PDF.
curl -sS -X POST "$HOST/api/batch" \
  -H 'content-type: application/json' \
  -d '{
    "template":"avery5163",
    "mode":"download",
    "labels":[
      {
        "option": {
          "orientation": "horizontal",
          "outline": "yes"
        },
        "data": {
          "id": "BOX.073",
          "url": "https://example.com/BOX.073",
          "name": "Floor Grinder",
          "tags": "Power tools",
          "description": "Angle grinder with floor grinding attachment and dust shroud"
        }
      }
    ]
  }' > "$OUT"

echo "Wrote $OUT"
