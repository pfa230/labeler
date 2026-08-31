#!/usr/bin/env bash
# Launch a long run detached, and wait on one (#284).
#
#   handle=$(.workflow/detach.sh <log-prefix> <command> [args...])
#   .workflow/detach.sh --wait "$handle" [seconds]
#
# The launch prints the handle on stdout and nothing else, so it can be captured; the
# friendly summary goes to stderr. The handle IS the log file.
#
# WHY IT EXISTS. AGENTS.md carried
#
#     setsid nohup timeout 5400 codex exec ... > "$raw" 2>&1 &
#
# and NEITHER setsid NOR timeout ships with macOS, which is what this repo is developed
# on. That line launches nothing and says so to nobody: `nohup` does report the missing
# binary and exits 127, but the `&` throws that away, because a shell reports 0 for
# having STARTED a background job whatever becomes of it. The result is a one-line
# capture indistinguishable from a clean pass. It cost a review round during #283.
#
# WHY DETACHED. A harness reaps its background tasks at turn boundaries: one run was
# killed 4.3 seconds after its turn ended, taking 15,127 lines of review with it, with
# no reason recorded and no way to tell that from a deliberate stop (#275). Surviving
# that needs a new SESSION, not merely immunity to SIGHUP, so `nohup` alone is the last
# resort rather than the macOS answer: setsid where it exists, else python3's setsid(),
# else nohup with that gap stated out loud.
#
# WHY EVERY LAUNCH GETS ITS OWN PATH. Earlier versions let two launches share one log,
# and tried to keep them apart with a pointer file and then an mkdir lock. Four review
# rounds found four more races in that machinery: a finished run's cleanup deleting a
# live run's claim, a killed launcher leaving a log claimed forever, a waiter reading a
# pointer a later launch had overwritten. None of them can exist here, because nothing
# is shared and nothing is reused. A handle is written once, by one run, and read by
# whoever holds it.
#
# WHAT THE LAUNCH TELLS YOU. It prints the handle and exits 0 when the run announced
# itself, and exits non-zero when it did not, having printed the handle anyway. The status
# is the signal, not the presence of the handle.
#
# WHAT IT DOES NOT PROMISE. A run that announces itself after the launch has given up
# still runs, and nothing here stops it: killing it would mean tracking a pid across a
# double fork, which is the machinery this deliberately does not have. Its handle records
# what it did, so a late run is discoverable rather than invisible - but a caller that
# gives up and retries can have two runs in flight, under two handles.
#
# HOW TO KNOW IT FINISHED. Not by the process existing: where setsid forks rather than
# execs, the pid this could report is a parent that exits at once while the real run is
# orphaned into another session. The run writes <handle>.exit when it ends, and
# <handle>.started as its first act, so a launcher that never got as far as running
# anything is reported rather than waited on. --wait has a deadline for the same reason
# the old line had `timeout`.
set -uo pipefail

usage='usage: handle=$(detach.sh <log-prefix> <command> [args...])   |   detach.sh --wait <handle> [seconds]'

# --- waiting -----------------------------------------------------------------------
if [ "${1:-}" = "--wait" ]; then
  [ "$#" -ge 2 ] && [ "$#" -le 3 ] || { echo "$usage" >&2; exit 2; }
  handle="$2"
  deadline="${3:-5400}"
  case "$deadline" in ''|*[!0-9]*) echo "the deadline is in seconds, got '$deadline'" >&2; exit 2 ;; esac
  [ -f "$handle" ] || { echo "no such run: $handle" >&2; exit 2; }

  # Never longer than the deadline itself: a short wait must still be able to report
  # that nothing started.
  grace=30
  [ "$deadline" -lt "$grace" ] && grace="$deadline"
  waited=0
  while [ ! -f "$handle.exit" ]; do
    # The run's own files are what this reports on. If they go, it has nothing to report
    # and says so rather than inventing a zero-line run or a launch that never started.
    [ -f "$handle" ] || { echo "the run's log vanished: $handle" >&2; exit 2; }
    if [ "$waited" -ge "$grace" ] && [ ! -f "$handle.started" ]; then
      echo "nothing has started within ${grace}s of the launch of $handle." >&2
      echo "Either the launcher never ran the command, or it is starting late and will" >&2
      echo "run: this cannot tell those apart, and $handle records it either way." >&2
      exit 1
    fi
    [ "$waited" -lt "$deadline" ] || {
      echo "still running after ${deadline}s: $handle" >&2
      echo "Judge it by the log growing; raise the deadline or stop it deliberately." >&2
      exit 1; }
    # Stepped so the last sleep never overshoots: at a deadline of 6 a fixed step of 5
    # would sleep to 10 and accept a run that finished at 7, past the bound it was given.
    step=5
    [ "$((deadline - waited))" -lt "$step" ] && step="$((deadline - waited))"
    [ "$step" -lt 1 ] && step=1
    sleep "$step"
    waited=$((waited + step))
  done

  status=$(tr -d '[:space:]' < "$handle.exit" 2>/dev/null)
  # `grep -c` prints 0 and EXITS 1 when it matches nothing, so `|| echo 0` would fire on
  # top of the 0 it already printed and the count would read "0\n0".
  [ -f "$handle" ] || { echo "the run finished but its log is gone: $handle" >&2; exit 2; }
  lines=$(grep -c '' "$handle" 2>/dev/null || true)
  printf 'finished: exit %s after %s lines\n' "${status:-unknown}" "${lines:-0}"
  case "$status" in
    ''|*[!0-9]*) echo "no usable exit status in $handle.exit (read '${status}')" >&2; exit 1 ;;
  esac
  # A wrapper can only ever record 0-255, so anything else is corrupt: `exit 999` would
  # silently become 231, a wrong status reported as a real one.
  [ "$status" -le 255 ] || { echo "impossible exit status '$status' in $handle.exit; a run records 0-255." >&2; exit 1; }
  exit "$status"
fi

# --- launching ---------------------------------------------------------------------
[ "$#" -ge 2 ] || { echo "$usage" >&2; exit 2; }
prefix="$1"; shift
command -v "$1" >/dev/null 2>&1 || { echo "$1 is not on PATH; nothing would run." >&2; exit 2; }

# mktemp creates the file atomically, so no two launches can pick one name: a timestamp
# plus $$ plus $RANDOM is only improbable, and testing for the name before creating it is
# a race in its own right.
handle=$(mktemp "$prefix.XXXXXXXX") || { echo "cannot create a handle under $prefix" >&2; exit 2; }

# $HANDLE is read inside the child at runtime, so the redirection and the status file
# are the child's own doing rather than this shell's. The status is written to a
# temporary and renamed, because a redirection creates the file before printf fills it
# and a waiter would otherwise read a real run as having no status at all.
body='printf "%s\n" started > "$HANDLE.started"
st=0; "$@" > "$HANDLE" 2>&1 < /dev/null || st=$?
printf "%s\n" "$st" > "$HANDLE.exit.part" && mv "$HANDLE.exit.part" "$HANDLE.exit"'

# POSIX sleep takes whole seconds; GNU and BSD both take fractions, and where neither
# does the poll simply ticks a second at a time.
TICK="${DETACH_TICK:-0.1}"
sleep "$TICK" 2>/dev/null || TICK=1

# Every input is settled BEFORE anything is spawned. Validating after the launcher has
# started a child means a bad value exits without printing the handle while the run goes
# on without one.
#
# How long a launch waits for its run to announce itself. Twenty seconds is generous for
# starting a process; the suite shortens it so a test of the give-up path does not cost
# twenty seconds of CI.
patience="${DETACH_PATIENCE:-20}"
case "$patience" in ''|*[!0-9]*) echo "DETACH_PATIENCE is in seconds, got '$patience'" >&2; exit 2 ;; esac
ticks=$(( patience * 10 ))
[ "$TICK" = "1" ] && ticks="$patience"
[ "$ticks" -lt 1 ] && ticks=1

launcher="${DETACH_LAUNCHER:-auto}"
case "$launcher" in
  auto)
    if command -v setsid >/dev/null 2>&1; then launcher='setsid'
    elif command -v python3 >/dev/null 2>&1; then launcher='python3'
    else launcher='nohup'; fi ;;
  setsid|python3|nohup) ;;
  *) echo "DETACH_LAUNCHER must be auto, setsid, python3 or nohup; got '$launcher'" >&2; exit 2 ;;
esac

case "$launcher" in
  setsid)
    HANDLE="$handle" setsid nohup bash -c "$body" _ "$@" > /dev/null 2>&1 & ;;
  python3)
    # Fork first: setsid() fails for a process group leader, and after a fork the child
    # never is one. A failure after that is real, so it is fatal rather than passed over.
    # Continuing would run the command in the session it was launched from, which is the
    # very thing this exists to avoid, while reporting success. stderr is /dev/null by
    # then and the intermediate parent has exited, so the log is the only channel back,
    # and leaving .started unwritten is what makes --wait report it.
    HANDLE="$handle" python3 -c 'import os, sys
if os.fork() > 0:
    os._exit(0)
try:
    os.setsid()
except OSError as e:
    try:
        open(os.environ["HANDLE"], "w").write("detach: setsid failed (%s); refusing to run undetached\n" % e)
    except OSError:
        pass
    os._exit(1)
os.execvp(sys.argv[1], sys.argv[1:])' bash -c "$body" _ "$@" > /dev/null 2>&1 & ;;
  nohup)
    echo "warning: no setsid and no python3, so this run is only SIGHUP-proof and can" >&2
    echo "still be reaped with its process group." >&2
    HANDLE="$handle" nohup bash -c "$body" _ "$@" > /dev/null 2>&1 & ;;
esac

# The launch does not return until the run has announced itself, or until it gives up
# waiting. Either way the handle is printed; the exit status is what distinguishes them.
announced=0
for _ in $(seq 1 "$ticks"); do
  [ -f "$handle.started" ] && { announced=1; break; }
  sleep "$TICK"
done
# Once more after the last sleep: the loop tests and then waits, so without this a child
# that announced itself during that final tick is reported as never having started.
[ -f "$handle.started" ] && announced=1
if [ "$announced" = "0" ]; then
  echo "nothing started within ${patience}s of launching $*" >&2
  echo "$handle may say why. This exits non-zero: check it, because the run may never" >&2
  echo "start, and --wait will otherwise sit there until its deadline." >&2
fi

# The handle is printed whatever happened, and the STATUS is what says whether the run had
# started. An earlier version withheld the handle on failure and gated the child on a
# go-ahead marker so a late child could not run unwatched. That machinery cost three
# ordering bugs in three review rounds - the marker written before the handle, a bound
# exported after the child was spawned, an unchecked printf releasing the child - to
# defend against a child that starts late and then runs correctly. A late run is not a
# wrong answer; a caller holding its handle can wait on it and get the truth. So the
# handle always goes out, and a launch that could not see its run start says so loudly
# and exits non-zero.
printf 'launched: %s\nvia:      %s\nwait:     %s --wait %s\n' "$*" "$launcher" "$0" "$handle" >&2
printf '%s\n' "$handle" || {
  echo "could not write the handle to stdout; the run is at $handle." >&2
  exit 1; }
[ "$announced" = "1" ] || exit 1
