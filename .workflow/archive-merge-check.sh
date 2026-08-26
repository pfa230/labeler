#!/usr/bin/env bash
# Merge-fidelity check for the archive step (#218).
#
# Archive resolves a change's delta specs into openspec/specs/ by locating each
# requirement BY NAME. A drifted or duplicated name rewrites the wrong requirement,
# silently, and nobody has reviewed that resolution: the plan review read the delta
# and never openspec/specs/. This replaces the archive-diff self-review, which asked
# whoever archived to eyeball a diff it had just produced and therefore could not fail.
#
# It checks the resolution, not the prose. The prose was reviewed at propose time and
# lands intact; what nobody has seen is which requirement got replaced.
#
# Usage: archive-merge-check.sh <repo-root> <changed-file>...
#
#   GATE_BASE_REF  ref holding the pre-change specs. Default HEAD, which is right for
#                  pre-commit. CI must set it to the push or PR base, because there
#                  HEAD *is* the commit under test and comparing it to itself passes
#                  everything.
#
# New content is read from the working tree, old content from the ref. Trailing
# whitespace and trailing blank lines are normalised away; nothing else is.
set -uo pipefail

root="${1:?repo root required}"; shift || true
[ "$#" -gt 0 ] || exit 0
base_ref="${GATE_BASE_REF:-HEAD}"

fail() { printf 'archive-merge: %s\n' "$1" >&2; failed=1; }
failed=0

tmp=$(mktemp -d) || exit 2
trap 'rm -rf "$tmp"' EXIT

# Split a spec into one file per requirement, printing "op<TAB>name<TAB>index".
# op is ADDED/MODIFIED/REMOVED in a delta, PLAIN in a published spec.
extract() {
  mkdir -p "$2"
  awk -v out="$2" '
    /^##[[:space:]]+(ADDED|MODIFIED|REMOVED)[[:space:]]+Requirements[[:space:]]*$/ { op = $2; if (f) { close(f); f = "" } next }
    /^###[[:space:]]+Requirement:/ {
      idx++
      name = substr($0, index($0, ":") + 1)
      gsub(/^[ \t]+|[ \t]+$/, "", name)
      f = out "/" idx ".body"
      printf "%s\t%s\t%s\n", (op == "" ? "PLAIN" : op), name, idx
      next
    }
    # op is cleared, not just the body: a requirement under some other level-two
    # header does not belong to the operation section that happened to precede it.
    /^##[[:space:]]/ { if (f) { close(f); f = "" } op = ""; next }
    { if (f != "") print > f }
  ' "$1"
}

# Compare two requirement bodies, ignoring trailing whitespace and trailing blanks.
norm() {
  sed -e 's/[[:space:]]*$//' "$1" | awk '{ l[NR] = $0 } END { last = NR; while (last > 0 && l[last] == "") last--; for (i = 1; i <= last; i++) print l[i] }'
}
same_body() { [ "$(norm "$1" | shasum -a 256 2>/dev/null || norm "$1" | sha256sum)" = "$(norm "$2" | shasum -a 256 2>/dev/null || norm "$2" | sha256sum)" ]; }

body_of() { # body_of <index-file> <name> -> path, or empty
  local idx dir="$2" name="$3"
  idx=$(awk -F'\t' -v n="$name" '$2 == n { print $3; exit }' "$1")
  [ -n "$idx" ] && printf '%s/%s.body' "$dir" "$idx"
}

# Capability of a spec path, published or delta: .../<cap>/spec.md
cap_of() { basename "$(dirname "$1")"; }
add_cap() { case " $caps " in *" $1 "*) ;; *) caps="$caps $1" ;; esac; }

# Both sides name capabilities, and both omissions matter. A published spec in the
# commit with no delta behind it was written by hand. A delta in the commit whose
# published spec is untouched was never synced, and "always sync every delta" is a
# rule. Only ARCHIVED deltas count: a planning commit legitimately adds a live change
# folder with nothing synced yet.
caps=""; published_changed=""; delta_caps=""
for f in "$@"; do
  case "$f" in
    openspec/specs/*/spec.md)
      cap=$(cap_of "$f"); add_cap "$cap"; published_changed="$published_changed $cap" ;;
  esac
done
for f in "$@"; do
  case "$f" in
    openspec/changes/archive/*/specs/*/spec.md)
      cap=$(cap_of "$f"); add_cap "$cap"; delta_caps="$delta_caps $cap" ;;
  esac
done
[ -n "${caps// /}" ] || exit "$failed"

for cap in $caps; do
  new="$root/openspec/specs/$cap/spec.md"

  delta=""
  for f in "$@"; do
    case "$f" in
      openspec/changes/*/specs/"$cap"/spec.md) delta="$root/$f" ;;
    esac
  done

  d="$tmp/$cap"; mkdir -p "$d/delta" "$d/new" "$d/old"
  if git -C "$root" show "$base_ref:openspec/specs/$cap/spec.md" > "$d/old.spec" 2>/dev/null; then
    extract "$d/old.spec" "$d/old" > "$d/old.idx"
    old_existed=1
  else
    : > "$d/old.idx"   # new capability: nothing existed to displace
    old_existed=0
  fi
  if [ -n "$delta" ] && [ -f "$delta" ]; then
    extract "$delta" "$d/delta" > "$d/delta.idx"
  else
    : > "$d/delta.idx"
  fi

  # The published spec is gone. That is a capability retired, which is legitimate only
  # if the delta removed every requirement it had: deleting the file takes the ones
  # nobody named with it. Checked here rather than in the branch below because
  # pre-commit's file list omits deletions and CI's does not, and the two must agree.
  if [ ! -f "$new" ]; then
    [ -n "$delta" ] || { fail "openspec/specs/$cap/spec.md was deleted with no delta behind it. The published specs are written by archive, never by hand."; continue; }
    if [ "$old_existed" = "0" ]; then
      fail "'$cap': a delta for it is being archived, but openspec/specs/$cap/spec.md exists neither in this commit nor at $base_ref. Nothing was synced."
      continue
    fi
    # Retiring a capability is the one reason its published spec may vanish, so the
    # delta has to be exactly that: removals, covering everything the spec had.
    # A flag, not `exit` in the rule: awk runs END afterwards, and END's exit status
    # is the one that survives, so the rule's verdict would be thrown away.
    if awk -F'\t' '$1 != "REMOVED" { bad = 1 } END { exit !bad }' "$d/delta.idx"; then
      fail "'$cap': the delta ADDs or MODIFIEs requirements, but openspec/specs/$cap/spec.md was deleted. Those requirements landed nowhere."
      continue
    fi
    while IFS=$'\t' read -r _ name _; do
      [ -n "$name" ] || continue
      awk -F'\t' -v n="$name" '$1 == "REMOVED" && $2 == n { found = 1 } END { exit !found }' "$d/delta.idx" \
        || fail "'$cap': the capability was retired, but no delta REMOVES \"$name\". Deleting the file takes requirements nobody named."
    done < "$d/old.idx"
    continue
  fi

  [ -n "$delta" ] || { fail "openspec/specs/$cap/spec.md changed, but this commit carries no delta for '$cap'. The published specs are written by archive, never by hand."; continue; }
  [ -f "$delta" ] || { fail "delta for '$cap' is in the commit but not on disk."; continue; }

  case " $published_changed " in
    *" $cap "*) ;;
    *) fail "'$cap': a delta for it is being archived, but openspec/specs/$cap/spec.md is not in this commit. Archive syncs every delta; this one was not synced."; continue ;;
  esac

  extract "$new" "$d/new" > "$d/new.idx"

  # 1. Every requirement the delta names landed as the delta wrote it, or is gone.
  while IFS=$'\t' read -r op name idx; do
    [ -n "$name" ] || continue
    dbody="$d/delta/$idx.body"
    nbody=$(body_of "$d/new.idx" "$d/new" "$name")
    case "$op" in
      REMOVED)
        [ -z "$nbody" ] || fail "'$cap': delta REMOVES \"$name\", but it is still in openspec/specs/$cap/spec.md." ;;
      ADDED|MODIFIED|PLAIN)
        if [ -z "$nbody" ]; then
          fail "'$cap': delta $op \"$name\", but no such requirement is in openspec/specs/$cap/spec.md. The name did not resolve."
        elif ! same_body "$dbody" "$nbody"; then
          fail "'$cap': \"$name\" differs between the delta and openspec/specs/$cap/spec.md. Archive rewrote what the review approved."
        fi ;;
    esac
  done < "$d/delta.idx"

  # 2. Every requirement the delta does NOT name survived untouched. This is the one
  #    that catches a bad match: a MODIFIED that resolved to the wrong requirement
  #    changes a requirement nobody named.
  while IFS=$'\t' read -r _ name idx; do
    [ -n "$name" ] || continue
    grep -q -F "	$name	" "$d/delta.idx" && continue
    obody="$d/old/$idx.body"
    nbody=$(body_of "$d/new.idx" "$d/new" "$name")
    if [ -z "$nbody" ]; then
      fail "'$cap': \"$name\" disappeared from openspec/specs/$cap/spec.md, and no delta removes it."
    elif ! same_body "$obody" "$nbody"; then
      fail "'$cap': \"$name\" changed in openspec/specs/$cap/spec.md, and no delta names it. Check which requirement the delta's MODIFIED resolved to."
    fi
  done < "$d/old.idx"
done

[ "$failed" = "0" ] || {
  echo "archive-merge: the published specs are not the delta applied to $base_ref." >&2
  exit 1
}
exit 0
