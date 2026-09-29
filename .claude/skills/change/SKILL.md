---
name: change
description: Turn an accepted GitHub issue into one openspec-loop run that creates, plans, reviews, implements, tests, archives and delivers the change, then report its outcome. Use when the user says `change ISSUE [context]` or asks to start the loop on an issue.
---

# change

`change <issue> [context]` starts one `openspec-loop run` for an issue and relays what it did. You are the operator: you invoke, wait and report. Never produce, edit, commit or review change content yourself, and never pass `--assume`.

Call the binary as `npx --no-install openspec-loop`, which resolves the project's development-dependency install. In the openspec-loop repository itself it resolves the package's own bin, `dist/bin.js`, which exists once `npm run build` has run.

Follow these steps in order.

## 1. Require the default branch

```sh
git branch --show-current
git symbolic-ref --short refs/remotes/origin/HEAD
```

The second prints `origin/<default>`. Unless the first prints `<default>`, stop and name the branch you are on. This skill starts changes; work already in flight continues only through the continuation command a run printed.

## 2. Read the issue

```sh
gh issue view <issue> --json number,title,body,state
```

Unless `state` is `OPEN`, stop before naming the change or creating any launch state. Report the issue's current state.

## 3. Name the change

Derive a name from the title that matches `^[a-z0-9]+(-[a-z0-9]+)*$` in at most 200 characters: lowercase letters and digits in words joined by single hyphens. State the name.

## 4. Question the requirements, then ask what would change scope or acceptance

For each requirement the issue states, its acceptance criteria and any direction it proposes included, name what breaks without it. Raise with the user, as a candidate to drop, any requirement with no nameable failure. Then ask the user about anything underspecified in the rest that would change scope or acceptance. Decide smaller details yourself. This is the only interactive step before launch.

## 5. Write the brief in a fresh launch directory

```sh
mkdir -p "$(git rev-parse --path-format=absolute --git-common-dir)/openspec-loop/runs"
launch="$(mktemp -d "$(git rev-parse --path-format=absolute --git-common-dir)/openspec-loop/runs/<name>-XXXXXX")"
printf '%s\n' "$launch"
```

The directory is outside every checkout. Keep the printed absolute path. Before launching, write `$launch/chain` holding only the launch directory's absolute path, encoded UTF-8, terminated by a single newline:
```sh
printf '%s\n' "$launch" >"$launch/chain"
```
Write `$launch/brief.md`: the issue's title and body, then the supplied context, then the answers from step 4 and your own decisions. `run` commits it as the change's `brief.md`.

## 6. Launch and wait in one shell tool call

From the invocation directory, issue this entire block as one shell tool call:

```sh
launch="<launch path printed by step 5>"
: >"$launch/stdout"
: >"$launch/stderr"
seen=0
setsid sh -c '
  npx --no-install openspec-loop run --change "$1" --input "$2/brief.md" --json \
    </dev/null >"$2/stdout" 2>"$2/stderr"
  echo $? >"$2/status.tmp" && mv "$2/status.tmp" "$2/status"
' sh "<name>" "$launch" </dev/null >/dev/null 2>&1 &
echo $! >"$launch/pid"
while :; do
  lines=$(wc -l <"$launch/stderr")
  if [ "$lines" -gt "$seen" ]; then
    sed -n "$((seen + 1)),${lines}p" "$launch/stderr" | grep '^progress: ' || true
  fi
  seen=$lines
  [ -e "$launch/status" ] && break
  if ! kill -0 "$(cat "$launch/pid")" 2>/dev/null && [ ! -e "$launch/status" ]; then
    echo interrupted
    break
  fi
  sleep 30
done
wait "$(cat "$launch/pid")" 2>/dev/null || true
```

Do not let the underlying shell session end while the run is active. `setsid` protects against a terminal hangup, but a managed tool runner may kill all descendants when a completed tool call is cleaned up. The foreground polling loop keeps that call alive. If Claude or Codex yields a live tool session or command handle, poll that same live tool session until this block completes. Never start a new shell merely to poll the files. `status` appears only once `run` has exited, because it is renamed into place.

## 7. Relay progress from that tool session

Relay every new line that begins with `progress: ` each round, and no other `stderr` line: each stage's start and end, and each participant, test or setup invocation's start and its end with outcome and elapsed seconds. A harness-required neutral heartbeat may state only that the same session remains active. Poll in bounded rounds if your harness limits how long one command may run. Participant, test and setup transcripts are in invocation logs, not in `stderr`. Read a log only where a failure needs it, only in the targeted range it needs, and find it by the path a `progress: tail <path>` line names, never by scanning `stderr`. A process that has ended with no `status` file is an interrupted launch: report it as interrupted, never as an outcome.

## 8. Investigate unsuccessful exits, report the outcome and stop

Read `$launch/status`:

Before reporting any unsuccessful exit, investigate it. An unsuccessful exit is status 1 or 2, a signal, an interrupted launch, or a launch that did not start. The emitted diagnostic is a lead, not the investigation.

Start with the structured result where one was emitted, the diagnostic at the end of `stderr`, and any targeted log named by a `progress: tail` line. Then use read-only checks tied directly to the reported condition to establish its current state. For example, inspect branch and checkout state for repository preflight failures, or the named lock, owning process and relevant worktree for contention. Never treat an earlier observation as current state. Do not scan unrelated logs.

Every unsuccessful report states the exit and stage, the cause and supporting evidence, the relevant current state, and the exact recovery with its preconditions when recovery is established. When recovery cannot yet be established, state what was checked, what remains unknown, and what evidence is required before recovery can be prescribed. Distinguish observed facts from inferences.

Investigation does not authorize cleanup, retry, continuation, branch changes, deletion or any other mutation. After the report, stop.

- **0** in `merge` mode: report the delivered result from `$launch/stdout`: its `deliver.mode` and `deliver.tip`; print the usage summary aggregated across the launch chain from `$launch/chain`:
  ```sh
  node -e '
  const fs = require("fs");
  const path = require("path");

  const chainPath = process.argv[1];
  const dirs = fs.readFileSync(chainPath, "utf8").trim().split("\n").filter((l) => l.length > 0);

  const items = [
    { label: "propose", stage: "propose", role: "propose" },
    { label: "tasks (propose)", stage: "tasks", role: "propose" },
    { label: "propose-review", stage: "propose", role: "propose-review" },
    { label: "implement", stage: "implement", role: "implement" },
    { label: "implement-review", stage: "implement", role: "implement-review" },
  ];

  const invocations = [];
  for (const dir of dirs) {
    const stdoutPath = path.join(dir, "stdout");
    const json = JSON.parse(fs.readFileSync(stdoutPath, "utf8"));
    if (Array.isArray(json.invocations)) invocations.push(...json.invocations);
  }

  const gatheredRows = items.flatMap((item) => {
    const instances = [...new Set(invocations
      .filter((inv) => inv.stage === item.stage && inv.role === item.role)
      .map((inv) => inv.instance))]
      .sort((a, b) => a < b ? -1 : a > b ? 1 : 0);
    return instances.map((instance) => ({
      item,
      instance,
      gathered: invocations.filter((inv) =>
        inv.stage === item.stage && inv.role === item.role && inv.instance === instance),
    }));
  });
  const allGathered = gatheredRows.flatMap(({ gathered }) => gathered);

  const cols = [
    {
      header: "Input tokens",
      has: (i) => typeof i.usage?.input_tokens === "number",
      val: (list) => String(list.reduce((sum, i) => sum + i.usage.input_tokens, 0)),
    },
    {
      header: "Output tokens",
      has: (i) => typeof i.usage?.output_tokens === "number",
      val: (list) => String(list.reduce((sum, i) => sum + i.usage.output_tokens, 0)),
    },
    {
      header: "Elapsed (s)",
      has: (i) => typeof i.wall_ms === "number",
      val: (list) => String(Math.floor(list.reduce((sum, i) => sum + i.wall_ms, 0) / 1000)),
    },
    {
      header: "Cost (USD)",
      has: (i) => typeof i.usage?.cost_usd === "number",
      val: (list) => String(list.reduce((sum, i) => sum + i.usage.cost_usd, 0)),
    },
  ];

  const activeCols = cols.filter((col) => allGathered.some((i) => col.has(i)));

  const headers = ["Line item", "Instance", "Invocations", ...activeCols.map((c) => c.header)];
  const delimiters = headers.map(() => "---");

  const lines = [
    `| ${headers.join(" | ")} |`,
    `| ${delimiters.join(" | ")} |`,
    ...gatheredRows.map(({ item, instance, gathered }) => {
      const cells = [
        item.label,
        instance,
        String(gathered.length),
        ...activeCols.map((col) => {
          const matches = gathered.filter((i) => col.has(i));
          return matches.length > 0 ? col.val(matches) : "";
        }),
      ];
      return `| ${cells.join(" | ")} |`;
    }),
  ];

  console.log(lines.join("\n"));
  ' "$launch/chain"
  ```
  close the issue with a comment naming `deliver.tip`:
  ```sh
  gh issue close <issue> --comment "Delivered in $(node -e 'console.log(JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).deliver.tip)' "$launch/stdout")"
  ```
  where the invoking checkout's `package.json` declares a `build` script, run `npm run build` in the invoking checkout so the next launch runs the delivered loop:
  ```sh
  node -e 'process.exit(JSON.parse(require("fs").readFileSync("package.json", "utf8")).scripts?.build ? 0 : 1)' && npm run build
  ```
  and remove the launch directory last:
  ```sh
  rm -rf "$launch"
  ```
  Each step runs in that order and only after the result is reported, a step's failure is reported without undoing an earlier step, and the launch directory is removed last whether the earlier steps succeeded or failed.
- **0** in `commit` or `pull-request` mode: report the delivered result from `$launch/stdout` (`deliver.mode` and `deliver.tip`), print the usage summary aggregated across the launch chain from `$launch/chain` with the aggregator above, and leave the issue open.
- **1** with `stop_reason` `needs-decision`: read the questions with `git show <name>:openspec/changes/<name>/questions.md`. The committed questions are the supporting evidence for the open-decision cause. Relay them and ask the user. The user's answers are the instruction to continue: launch the continuation as below, writing the answers to `answers.md` in the new launch directory and adding `--input <new launch>/answers.md` to the command printed at the end of the previous launch's `stderr`.
- **1** otherwise: report `stop_reason`, `stage`, and the record (`propose.record` or `implement.record`) or the failed test result (`test.command`, `test.exit_status`, `test.head`) and the log the `progress: tail` line names, then stop. A review cap wants a person, not another round.
- **2**: report the diagnostic at the end of `$launch/stderr`, the log a `progress: tail` line names where there is one, and the continuation or recovery the diagnostic names, then stop.
- A launch that did not start, and any other status, including a signal's: report an operational failure naming `$launch`, then stop.

Never run a continuation unasked. A resume is the operator's explicit act, and the user's instruction to continue is that act. Then make a fresh launch directory with step 5's `mktemp`, writing no brief there, only the answers step 8 names. Before launching, copy the stopping launch directory's `chain` to the fresh launch directory and append the fresh directory's absolute path as one further newline-terminated line:
```sh
cp "$prev_launch/chain" "$launch/chain" && printf '%s\n' "$launch" >>"$launch/chain"
```
so the chain runs from the initial launch to that launch's own directory, encoded UTF-8, with each line terminated by a single newline and no other line. Then launch the printed command as in step 6, calling `npx --no-install openspec-loop` in place of `openspec-loop` and writing `stdout`, `stderr`, `status` and `pid` into the new directory.
