# PR Resolver

A desktop app for selectively promoting merged pull requests between branches —
without merging entire branches, and without losing track of what's already moved.

## 1. The problem

Teams running a multi-stage branch pipeline (e.g. `develop` → `staging` → `prod`)
often can't promote everything that's merged into the upstream branch at once.
Some merged PRs are ready to go further; others need to sit and wait. Git only
understands commits and branches — it has no concept of "PR", and once a PR is
merged its commits are just part of the branch's linear history. Promoting only
some of them means cherry-picking, and cherry-picking loses the PR-level view
entirely: after a cherry-pick, the commit SHA changes, so there's no simple way
to check "has this PR already been promoted?" by comparing SHAs.

Real precedent for this exact shape of problem: Kubernetes' release cherry-pick
bot, which lets maintainers selectively backport individual merged PRs from
`master` to `release-1.XX` branches, tracked via labels and automated PR
creation. `git-pr-release` (Ruby gem) solves a narrower version of the same
problem. PR Resolver generalizes this into a standalone tool for **any**
source/target branch pair, on **any** repo.

## 2. What it does

1. Connect a repo — either a GitHub repo (via OAuth) or a local git repo on disk.
2. Define one or more **pipelines**: a `(repo, source_branch, target_branch)`
   triple. A repo can have many pipelines (`develop→staging`, `staging→prod`,
   `feature/x→main`, etc). Branch names are never hardcoded.
3. The app syncs merged PRs on the source branch and determines, per pipeline,
   which of them are **not yet present** on the target branch — tracked at PR
   granularity, using content-based patch matching so it stays correct across
   cherry-picks.
4. The user gets a checklist: tick the PRs they want promoted now, leave the
   rest.
5. The app cherry-picks the selected PRs' commits onto a new branch off the
   target branch, in merge order.
6. On conflict, the app stops, surfaces the conflicting PR + files, and offers
   to open the working copy directly in the user's preferred code editor
   (VS Code, Cursor, Zed, or others detected on the system) for manual
   resolution, then resumes the cherry-pick sequence.
7. Once all selected PRs are applied cleanly, the app pushes the new branch
   and opens a PR to the target branch (remote repos), or leaves the branch
   ready to merge locally (local-only repos), with an auto-generated
   description listing exactly which PRs are included.
8. Everything not selected stays exactly where it was — still on the source
   branch, still available for a future promotion run.

## 3. Non-goals (v1)

- Not a general git GUI — no arbitrary rebasing, no history rewriting UI.
- Not a CI/CD orchestrator — it doesn't run builds or deploy anything, only
  produces the branch/PR that a normal pipeline would then build.
- Not multi-user/collaborative in v1 — each install is single-user, local-first.
  A hosted multi-tenant web version is an explicit v2, not v1 scope.
- Not opinionated about merge strategy — merge commits, squash, and rebase
  merges are all supported, but squash merges make PR↔commit mapping trivial
  (1:1) while merge commits and rebase merges need the fuller patch-id logic.

## 4. Repo connection modes

Two modes, both first-class, chosen per repo:

**Remote (GitHub) mode**
- Auth via GitHub OAuth **device flow** (same UX as the `gh` CLI: user gets a
  code, enters it at github.com/login/device — no embedded browser, no
  redirect URI to manage in a desktop app).
- Token stored in the OS keychain (macOS Keychain / Windows Credential
  Manager via the `keyring` crate) — never written to disk in plaintext, and
  never touches the SQLite database.
- After auth: list the user's orgs → repos → branches via the GitHub API.
- The app manages its own clone of the repo in an app-data directory
  (`managed_clone_path` in the schema) — this keeps the tool independent of
  whatever state the user's own working copy is in.
- PR metadata (title, author, merge strategy, ticket refs, merge commit SHA)
  comes from the GitHub API.
- Promotion runs end by opening a real PR on GitHub via the API.

**Local mode**
- The user points the app directly at an existing local git repo
  (`local_path` in the schema) — no GitHub account or API access required.
- Source/target branches are just local (or local-tracking-remote) branches.
- There's no "PR" concept from an API — a PR is inferred either from merge
  commits with `--no-ff` (each merge commit = one logical unit to
  promote/skip) or, if the user's workflow uses squash merges, from
  individual commits on the source branch. The checklist UI in local mode
  operates on these logical units the same way it operates on GitHub PRs.
- Promotion runs end with a ready-to-merge local branch — no remote PR is
  opened, since there may be no remote at all.

Both modes share the same downstream pipeline definition, checklist UI,
cherry-pick engine, and conflict flow. The only thing that differs is where PR
metadata comes from and what "done" means at the end of a promotion run.

### Working copy: managed clone vs. existing local clone

Separately from where PR metadata comes from, the user chooses **where the
working copy lives**, independent of remote/local mode:

- **Managed clone** — the app clones and maintains its own copy in an
  app-data directory. Keeps the tool isolated from whatever state the user's
  own checkout is in.
- **Point at an existing local clone** — the user already has the repo
  checked out somewhere (their normal working directory) and wants the app
  to operate there directly instead of maintaining a separate copy.

Both options are available for a GitHub-connected (remote) repo — remote mode
determines where *PR metadata* comes from, working copy mode determines where
*git operations* happen. They're independent settings, not tied together.
`repos.working_copy_mode` in the schema captures this.

## 5. Data layer

**Storage: SQLite, local-first.** All pipeline configuration, PR/commit
metadata, and promotion history lives in a local SQLite file — the user's
data never leaves their machine unless they explicitly connect a remote repo
(and even then, only repo/PR metadata is fetched from GitHub; nothing is sent
elsewhere).

Use `sqlx` (or `sea-orm`) as the data-access layer rather than hand-written
per-database queries. Both support SQLite and Postgres behind the same query
interface, which is what makes a later hosted web version (v2) a storage-swap
rather than a rewrite: same schema shape, same access layer, SQLite → Postgres,
polling → webhooks.

See `schema.sql` (attached alongside this document) for the full DDL. Summary
of tables:

| Table | Purpose |
|---|---|
| `repos` | A connected repo, remote or local, plus its managed clone path |
| `pipelines` | A `(repo, source_branch, target_branch)` promotion pipeline |
| `branch_sync_state` | Last-synced SHAs per pipeline, for incremental sync |
| `pull_requests` | Merged PR metadata, scoped to a repo |
| `pr_commits` | Commits per PR with content-based `patch_id` |
| `promotions` | Per-pipeline status of a PR (pending/selected/promoted/conflict/skipped) |
| `promotion_runs` | One "promote selected PRs" action → one branch/PR |
| `promotion_run_prs` | Which PRs went into a given run |
| `conflicts` | A cherry-pick conflict within a run, and how it was resolved |
| `editors` | Detected/registered code editors for conflict resolution |
| `settings` | Non-secret app config (secrets live in the OS keychain, not here) |

## 6. Sync engine

Since this is a local-first desktop app (not a server), there's no webhook
endpoint. **v1 sync is manual only** — the user hits "refresh" on a pipeline
and the app syncs that pipeline's source/target branches on demand. No
background polling, no interval to configure.

- **Primary: GraphQL.** Batch-fetch commits, associated PRs, and metadata in
  as few round trips as possible — this matters at scale, since the whole
  point is supporting "as many PRs as possible" across potentially many repos.
- **Fallback: REST.** Anything GraphQL doesn't cleanly expose (some diff/patch
  content, certain edge cases in commit-to-PR association) falls back to REST
  calls.
- **Incremental by design.** `branch_sync_state` tracks the last-synced SHA
  per branch per pipeline. Each sync only walks commits between the last
  known head and the current head — never the whole branch history — and
  only computes `patch_id` for genuinely new commits. This is what keeps sync
  cost roughly proportional to *new* activity rather than repo size, even
  though it's user-triggered rather than automatic.
- **Conditional requests.** Use ETags where the GitHub API supports them so
  unchanged resources don't count against the rate limit.
- **Rate limits.** Authenticated REST is 5,000 requests/hour; GraphQL has its
  own point-based budget. Manual-only sync keeps this comfortably out of
  reach for v1 usage patterns.

Background polling and webhook-driven live sync are both out of scope for
v1 — see Section 11 (Roadmap).

## 7. Cherry-pick / promotion engine

Given a pipeline and a set of user-selected PRs:

1. Ensure the managed clone (or the user's pointed-at local repo) is fetched
   up to date on both source and target branches.
2. Create a new branch off the current tip of `target_branch`.
3. Cherry-pick each selected PR's commit(s) onto the new branch, **in merge
   order** (oldest first), so later PRs that depend on earlier ones apply
   correctly.
4. On a clean cherry-pick: record the new commit SHA in `promotions`, move
   its status to `promoted`.
5. On conflict: stop the sequence, record a `conflicts` row per conflicting
   file, mark the promotion `conflict`, and hand off to the conflict
   resolution flow (Section 8). The rest of the selected PRs stay queued —
   don't attempt to skip ahead automatically.
6. Once every selected PR in the run is cleanly applied: push the branch
   (remote mode) and open a PR via the GitHub API with an auto-generated body
   listing the included PR numbers/titles/tickets, or leave the branch ready
   to merge (local mode).
7. Anything not selected is left untouched on the source branch — no branch
   state is ever mutated except the new promotion branch.

**Dependency awareness (soft check, not a hard gate):** if a selected PR
touches files also touched by an *unselected* PR that sits earlier in the
merge order, warn the user before starting the run rather than blocking them
— they may know it's safe. This is a warning, not a rule.

## 8. Conflict resolution: editor handoff

When a cherry-pick conflicts:

- Detect installed editors by checking common CLI launchers on `PATH`:
  `code` (VS Code), `cursor` (Cursor), `zed` (Zed), extensible to others.
  On macOS, also fall back to `open -a "<App Name>"` for editors that install
  as `.app` bundles without a CLI symlink.
- Store detected editors in the `editors` table, with one marked as
  `is_preferred`.
- Provide an explicit **"detect editors again"** action in settings — the
  user can install a new editor after first launch and re-run detection
  without reinstalling or restarting the app.
- On conflict, launch the preferred (or user-chosen, if none set as default)
  editor directly on the working copy / conflicting file path.
- The app polls (or the user manually confirms) when conflicts in that file
  are resolved, marks the `conflicts` row `resolved`, and resumes the
  cherry-pick sequence for the remaining selected PRs.
- If the user abandons resolution, the whole run can be aborted cleanly: the
  in-progress branch is discarded, and every promotion in that run reverts to
  its pre-run status (still `selected`, not `promoted`) so nothing is lost
  and the next attempt starts fresh.

## 9. Core UI flows

1. **Connect** — choose remote (GitHub OAuth device flow → org → repo) or
   local (file picker to an existing repo).
2. **Pipeline setup** — pick source and target branch, name the pipeline.
   A repo can have multiple pipelines; all are listed on a dashboard.
3. **Checklist** — for a selected pipeline: list of merged PRs on the source
   branch not yet present on the target branch (per patch-id matching), each
   with title, author, ticket ref if detected, merge date. Multi-select
   checkboxes, "select all", "promote selected" action.
4. **Promotion run** — progress view as PRs are cherry-picked in order; stops
   and surfaces detail immediately on conflict.
5. **Conflict resolution** — file list, "open in [editor]" button per file,
   "mark resolved and continue" / "abort run" actions.
6. **History** — past promotion runs per pipeline: which PRs went in, when,
   link to the resulting PR (remote) or branch (local).
7. **Settings** — a dedicated page covering:
   - **Accounts** — connected GitHub accounts, disconnect/reconnect, token status
   - **Merge behavior** — default merge strategy assumption (merge/squash/
     rebase) per repo, since it changes how PR↔commit mapping is inferred
   - **Appearance** — light/dark/system theme
   - **Working copy** — per-repo managed-clone-vs-existing-local-clone choice,
     and where managed clones live on disk
   - **Editors** — detected editors list, preferred default, a "re-detect
     editors" action for when a new editor is installed after first launch
   - **Notifications** — alert on conflict, alert on promotion run completion
   - **Data & privacy** — clear local cache/database, reset app state
   - **Diagnostics** — API rate-limit usage indicator

## 10. Packaging & tech stack

- **Framework: Tauri.** Rust backend for all git operations (via `git2-rs` or
  shelling out to the system `git` binary — evaluate both; `git2-rs` avoids a
  system git dependency but shelling out is simpler for parity with real git
  CLI behavior, including cherry-pick conflict semantics), web frontend for
  the UI (framework choice open — React/Vue/Svelte all work fine under
  Tauri).
- **Cross-platform build:** macOS and Windows from one codebase (Tauri
  supports Linux too, worth enabling even if not an initial target).
- **Data:** SQLite via `sqlx`/`sea-orm`, file stored in the OS app-data
  directory.
- **Secrets:** OS keychain via the `keyring` crate — GitHub tokens never
  touch the SQLite file or disk in plaintext.
- **Why not Electron:** smaller binary, and the Rust backend is a better fit
  for the actual git-plumbing-heavy nature of this tool (this is also a
  deliberate skills-building choice for the project owner, not just a
  technical one — worth preserving even though it's not a hard requirement).

## 11. Roadmap

- **v1 (this spec):** local-first desktop app, SQLite, GitHub OAuth device
  flow + local repo mode, single pipeline type, editor handoff for conflicts.
- **v2:** hosted web version — same schema/access-layer shape ported to
  Postgres, GitHub App + webhooks (plus optional background/interval sync)
  replacing manual-only refresh, multi-user/team accounts, shared promotion
  history and audit log.
- **Later, open-ended:** support for other hosts (GitLab, Bitbucket) behind
  the same repo-connection abstraction; richer dependency detection between
  PRs; scheduled/automatic promotion rules (e.g. "auto-promote PRs older than
  48h with no conflicts").

## 12. Open questions for the build

These are the assumptions made in this spec that are worth confirming or
revisiting once implementation starts, rather than blocking on:

- Merge-order tie-breaking when two PRs were merged at effectively the same
  time — default to merge commit timestamp, fall back to PR number.
- Whether `git2-rs` or shelling out to system `git` is the better default for
  cherry-pick — recommend prototyping both against a repo with real merge
  conflicts before committing.