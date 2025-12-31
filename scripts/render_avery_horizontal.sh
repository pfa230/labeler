#!/usr/bin/env bash
set -euo pipefail

HOST=${HOST:-http://localhost:8080}
OUT=${OUT:-avery-horizontal.pdf}

curl -sS -X POST "$HOST/render/batch" \
  -H 'content-type: application/json' \
  -d '{
    "template":"avery5163",
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
