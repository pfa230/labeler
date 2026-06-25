#!/bin/sh
# Run the service as PUID:PGID (default 1000:1000), the homelab convention (linuxserver-style), so
# named volumes and host bind-mounts line up with the operator's user. The container starts as root,
# fixes ownership of the writable dirs, then drops privileges via gosu. See ADR-0029.
set -e

PUID="${PUID:-1000}"
PGID="${PGID:-1000}"

chown -R "${PUID}:${PGID}" "${LABELER_DATA_DIR:-/app/data}" "${LABELER_TEMPLATES_DIR:-/app/templates}"

exec gosu "${PUID}:${PGID}" /app/labeler "$@"
