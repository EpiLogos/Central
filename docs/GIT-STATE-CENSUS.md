# Git State Census — branches and worktrees as a read model

```yaml
standing: design-spec
register: implementation
provenance: agent-proposed
temporal: stages 1-3 built 2026-09-17 on techne/git-state-census; stage 4 skill/wiki deltas remain procedural
date: 2026-09-17
```

**Ordinary correctness:** no QL, Factory, O:I, database, session or chat dependency.

## Build status (2026-09-17)

Built on `techne/git-state-census`: `GIT_STATE_PORT` + `GitState` in the SDK,
implemented by `personal.git-sync` (now mounted in the default Connector
registry, which also activates the previously dead `SourceHistory` port),
`central.git.census` with list/tree/graph renders and `ctrl git` arms, lane
attribution (`.aikit/tasks`, clearing and handoff `work_refs`), the attention
list, `work_refs` on both NOW schemas (clearing uses `central.now-clearing/v2`
when present; readers accept both), and census capture + reconciliation in
`projectcentral.now.rollover` (report `git_census`/`open_lanes`, full document
written beside the day record as `git-census.json`). Remaining: the three
skill deltas and the wiki learning below stay procedural until adopted, and
the named gaps stand.

## The problem this solves

Open branches and cut worktrees are currently invisible to the tooling. The
state lives only in `git` invocations run by whoever remembers to run them, and
in prose inside NOW records. The consequence, observed on 2026-09-17 across the
personal root's repositories:

```text
O-I               worktrees=10  branches=43  commits on no remote=19  dirty=134
Quaternal-Logic   worktrees=14  branches=22  commits on no remote=6   dirty=21
legacy/Epi-Logos  worktrees=4   branches=14  commits on no remote=14  dirty=719
ai-kit            worktrees=3   branches=8   commits on no remote=6
Actuation         worktrees=2   branches=9   commits on no remote=5
...18 repositories total, ~50 worktrees, ~140 local branches
```

Every one of those "commits on no remote" numbers is work that exists on one
machine only, held by a tree that may or may not be remembered. The failure
mode is well known here: agents mint fresh trees because they cannot see
existing lanes; lanes outlive their sessions unrecorded; convergence becomes
archaeology.

## Is this feature already integrated?

No. There is no git-state surface in `ctrl` today. What exists is scattered:

- `aikit task` (ai-kit repo) cuts worktrees under `<repo>/.aikit/tasks/<name>`,
  and its teardown refuses dirty or unpushed trees (`worktree_blockers`). Its
  module documentation already treats the **shared** tree as the normal case
  and the worktree as the isolation variant — the reuse behaviour the owner
  wants is already modelled there. What it lacks is a cross-project read
  surface: it only ever speaks about the repo it was invoked in.
- The skills (`agent-worktree-lifecycle`, `central-git-convergence`,
  `central-parallel-execution`) are procedures that instruct agents to inspect
  before minting, continue existing lanes, and record returns — with no tool
  backing the inspection. Detection currently relies on every agent re-deriving
  `git worktree list` by hand.
- `connectors/git-sync` already shells out to real `git` (including the
  `.git`-as-file worktree case) behind the SDK's
  `SYNCHRONIZER_PORT` / `SOURCE_HISTORY_PORT` — the seam for reading git state
  exists and is proven.
- `ctrl/src/world_map.rs` carries a dead `uncommitted_edits` field that nothing
  produces — an earlier anticipation of exactly this feature, never wired.

## The feature

One read model, one action family, three projections. Strictly read-only.

### Action family

```text
central.git.census   {"scope": {"project": "O-I" | absent = all registered repos},
                      "format": "json" | "list" | "tree" | "graph",
                      "include_paths": false}
```

- Root register (`central.*`, absent `project` = every repository under the
  Work root plus the Control repo, discovered by walking for `.git` at depth
  ≤ 3, stable sort by path). With `project`, one repository.
- `mutation_class`: read-only. Census never fetches, never prunes, never
  touches git state. A stale `origin` ref set is reported as-is; the
  convergence skill already owns `git fetch --all --prune` before inspection.
- Registered like any action family (`register_git_actions` in the `ctrl`
  aggregator), discoverable via `ctrl actions`; convenience parse arms
  `ctrl git census [project]`, `ctrl git tree [project]`,
  `ctrl git graph [project]` added to `parse_args` and `CLI-REFERENCE.md`.

### Data model — `central.git-census/v1`

Per repository:

```json
{
  "repo": "Work/O-I",
  "default_branch": "main",
  "remote": "git@github.com:EpiLogos/O-I.git",
  "remote_refs_fetched_at_git_refs_only": true,
  "worktrees": [
    {
      "path": "Work/O-I",
      "branch": "main",
      "detached": false,
      "locked": false,
      "prunable": false,
      "dirty_files": 134,
      "ahead": 2, "behind": 0,
      "last_commit_at": "2026-09-17T09:14:00+01:00",
      "lane": "aikit-task:<name> | harness-recorded | unattributed"
    }
  ],
  "branches": [
    {
      "name": "thread4/nara-speech-experience",
      "checked_out_in": ["Work/O-I/.aikit/tasks/nara-walk"],
      "upstream": "origin/thread4/nara-speech-experience",
      "ahead": 3, "behind": 0,
      "local_only": false,
      "last_commit_at": "...",
      "stale_days": 0
    }
  ],
  "unmerged_tips": ["...branch names whose tip is reachable from no remote ref..."],
  "generated_at": "..."
}
```

Field law:

- `local_only` — the branch tip is contained in no remote ref. These are the
  machine-lost-work candidates and get emphasis in every projection.
- `stale_days` — not checked out in any worktree AND no commit in N days
  (default N=14). A stale flag is information, never a deletion authorisation.
- `prunable` — `git worktree list` reports the directory missing or stale.
  Reported, never auto-pruned.
- `lane` attribution, in confidence order: (a) the worktree lives under
  `<repo>/.aikit/tasks/` → the aikit task name; (b) an active NOW clearing in
  the owning register references the branch through its `work_refs` field
  (see NOW integration) → the now_ref; (c) otherwise `unattributed`. The
  census never guesses by matching prose — an unattributed lane is reported as
  unattributed.
- Dirty state is a count by default. `include_paths: true` adds changed paths
  (never content, never env/secret values), for convergence sessions only.

### Detection implementation

A new `GIT_STATE_PORT` in `crates/connector-sdk` (same `PortContract` pattern
as `SOURCE_HISTORY_PORT`, with a conformance harness), implemented by the
existing `personal.git-sync` connector, which already shells out to `git` and
handles the worktree `.git` pointer case. No `git2` dependency is added.

Per repository, the port executes only:

```text
git worktree list --porcelain
git for-each-ref refs/heads --format=<atoms incl. %(upstream:track), %(committerdate)>
git rev-list --branches --not --remotes --no-walk        (unmerged-tip fast path)
git status --porcelain                    (per checked-out worktree, count only)
git config --get remote.origin.url ; git symbolic-ref refs/heads/<default>
```

Unmerged membership is verified per tip with `git branch -r --contains` only
for the tips the fast path names, so a large repo costs a handful of git calls.
Unborn repositories (no commits yet) and bare inputs are reported as empty
census entries, not errors.

### Projections

JSON is canonical; the other formats are renders of the same document.

- `list` (default human output, `human_output` formatter): one line per repo —
  `O-I: worktrees=10 branches=43 local_only=19 dirty=134` — then per worktree
  and per local-only branch, one line each, `unattributed` flagged.
- `tree`: project → repository → worktrees (path, branch, ahead/behind, dirty,
  lane) → parked branches (not checked out), local-only branches marked `!`.
- `graph`: a mermaid document — one node per repository (linked to its remote
  node), one node per worktree and per local-only branch, `checked-out` edges,
  `ahead-N` labels on unmerged edges. Mermaid renders in GitHub and markdown
  surfaces; DOT can be added later behind the same format parameter if wanted.

## Integrations

### Agent tree-minting and git use (skill deltas — owner adoption)

The visibility tool only pays off if the minting rules point at it. Three
deltas, proposed to the owning skills (not code):

1. `agent-worktree-lifecycle` — **reuse before mint**: before
   `aikit task spawn --worktree`, run the census for the repo. If a lane for
   the same subject exists and is clean and idle, join it (shared isolation)
   or continue its branch; mint only when no lane fits or the work is
   genuinely independent. The current text prefers the aikit path but never
   says *check what already exists first*.
2. `central-git-convergence` — "Before development" gains one line: the census
   action is the canonical inspection; `git worktree list` by hand is the
   fallback, not the habit.
3. Returns: a session that cut or occupied a lane records repo + branch +
   worktree path in its return (already the convention in prose; becomes
   checkable with the field below).

### NOW/DAY integration (one schema change — owner adoption)

- **Clearing schema**: add optional `work_refs: [{repo, branch, worktree_path}]`
  to `central.now-clearing/v1`. The schema is `deny_unknown_fields`, so this is
  a deliberate schema revision, not a silent additive field. Returns already
  name branches in prose; this makes them machine-checkable. Returns without
  the field remain valid; their lanes simply read `unattributed`.
- **Day close**: the close's snapshot step captures the census document for the
  register's repos, so every dated day record carries "what was open at close".
  The derive step then emits a reconciliation into the reading: open lanes
  versus lanes referenced by carried records — every `unattributed` worktree or
  `local_only` branch surfaces as an open question for tomorrow, instead of
  surfacing three weeks later as a merge conflict. (The 2026-09-17
  suite-harmonised-cut NOW record is this reconciliation done by hand; the
  point is to make it mechanical.)
- **Ledger**: the census is evidence pointed to, not retold — writing budgets
  stay with `central-ledger`.

### Wiki

Live git state never enters `wiki.json` — it would rot on the first commit and
the wiki's own law forbids becoming a stale mirror. The wiki receives one
*learning* through the normal return door: where lane state lives
(`central.git.census`), what `local_only` and `unattributed` mean, and the
reuse-before-mint rule. Dated census snapshots from day closes are the citable
artifacts when a wiki entry needs to point at historical state — the day
records are retained, so the citations stay stable.

## Staging

1. **Port + core action** — `GIT_STATE_PORT`, git-sync implementation,
   `central.git.census` JSON, contract tests against a synthetic fixture repo
   (cut worktrees, assert census matches `git worktree list` exactly, assert
   prunable/unborn/local_only handling). Accept: fixture repo goldens byte-pin
   the JSON.
2. **Projections** — list/tree/graph renders + `parse_args` arms +
   `CLI-REFERENCE.md` entries. Accept: contract test pins one golden per
   format over the fixture repo.
3. **Attribution + reconciliation read** — `.aikit/tasks` lane tagging;
   unattributed-lane listing; `ctrl central.day.lifecycle` census snapshot +
   reconciliation (root) and the same via `projectcentral.now.rollover`
   (project). Accept: day-close test shows an unrecorded lane surfacing as an
   open question.
4. **Schema + skills + wiki return** — `work_refs` clearing field; the three
   skill deltas; wiki learning via return. Accept: end-to-end — a session
   return with `work_refs` makes its lane attributed in the next census.

## Named gaps (explicitly out of scope here)

- **Cross-machine lanes.** Census sees the local machine. Worktrees that exist
  only on Omarchy are invisible until their branches are pushed; `local_only`
  marks the inverse risk. A remote census (ssh to the Workcell host) is a
  follow-up operation behind the same port contract.
- **PR / review state.** Open-PR data needs `gh` and network; it is a separate
  operation, not census. Branch-level facts stay decoupled from GitHub state.
- **Mutation.** Prune, remove, delete stay exactly where the lifecycle skills
  put them: decisions with owners. Census reports; it never cleans.

## Owner decision points

1. `work_refs` clearing-schema field (the only schema change in the spec).
2. Mermaid as the graph format (default choice) vs DOT.
3. Whether `ctrl git census` should also accept the bare phrase
   `ctrl lanes <project>` as an alias for agent ergonomics.
