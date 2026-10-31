# ftor-calls

## Commits are the release notes

`run-release` writes each version's notes from the commit subjects since the last tag
(`scripts/publish.sh`: `git log --no-merges --format='- %s'`), and ftor's apps page shows them
under that version, with the kind in its own column. Every subject on main is a line of the
changelog someone reads before updating — write it as one.

- **Conventional Commits**: `<kind>(<scope>): <what changed>`. Kinds: `feat` (something new
  the app does), `fix` (something it did wrong and no longer does), `perf`, `refactor`,
  `build`, `ci`, `test`, `docs`, `chore`. Scope is optional: the part of the app
  (`notion`, `tray`, `usage`).
- **Say the change, not the work.** Name the behaviour as the person using the app sees it,
  specific enough to be checked: `fix(notion): export the call's time in the machine's zone`,
  not `fix bug`, `improvements`, `beautify`, `settings` or `refactor code for readability`.
- Lowercase after the colon, no trailing period, one line of 72 characters or less.
- **One change per commit.** A change that needs "and" to describe is two commits.
- **Breaking**: `feat!:` / `fix!:` plus a `BREAKING CHANGE: <what the user must do>` footer
  when an update loses or moves something they relied on (settings, saved data, a shortcut).
- **No version numbers in commits.** `run-release` bumps `tauri.conf.json` and commits
  `release vX.Y.Z` itself (left out of the notes); never edit the version by hand.

## Choosing the bump

`run-release` asks which part of the version goes up. Read it off the commits since the last tag
(`git log --oneline $(git describe --tags --abbrev=0)..HEAD`): any `!` → `major`, any `feat`
→ `minor`, otherwise `patch`.
