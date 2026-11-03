## Git Workflow

- This repo uses a bare repo + git worktree layout (`.bare`, with `main`/`develop`/feature worktrees as siblings).
- `feature/*` branches are always branched from `develop`, not `main`.
- The workspace root has no `.git`; run git commands from inside a worktree, targeting `../.bare`.
- When a skill (e.g. `finishing-a-development-branch`) asks whether to merge into `main`, merge into `develop` instead unless the user explicitly says `main`.

## Creating a worktree (overrides superpowers:using-git-worktrees defaults)

- For new feature work, ALWAYS create a new worktree, even if the current directory is already a linked worktree (skip the skill's Step 0 "already isolated" rule).
- Do NOT use `EnterWorktree` or a `.worktrees/` directory. Do not modify `.gitignore` for worktrees.
- Steps:
  1. Update local develop: `git -C ../develop pull --ff-only origin develop`
  2. Create: `git -C ../.bare worktree add -b <type>/<name> ../<type>-<name> develop`
     - Directory name = branch name with `/` replaced by `-` (`feature/deploy` → `feature-deploy`).

## Finishing a feature branch ("커밋 후 정리해줘")

When the user asks to "commit and clean up" (e.g. "커밋 후 정리해줘") from a feature worktree, do all of the following in order:

1. Commit the changes.
2. Merge into `develop` with `--no-ff` (run from the `develop` worktree). If the merge fails or conflicts, stop and report — keep the branch and worktree untouched.
3. Only after a successful merge, remove the feature worktree (`git worktree remove ../<type>-<name>`) and delete the local branch (`git branch -d <type>/<name>`).
4. Do NOT push. Push only when the user asks separately.

## Delegating to other agents (herdr)

- When calling another agent (Agent tool subagent, Codex, etc.) and `HERDR_ENV=1` (running inside herdr), use the `herdr` skill to open a new pane and run the agent there so the user can watch its progress.
- Tell the user which pane was created. If `HERDR_ENV` is not set, call the agent normally.

## Skill routing

When the user's request matches an available skill, invoke it via the Skill tool. When in doubt, invoke the skill.

Key routing rules:

- Product ideas/brainstorming → invoke /office-hours
- Strategy/scope → invoke /plan-ceo-review
- Architecture → invoke /plan-eng-review
- Design system/plan review → invoke /design-consultation or /plan-design-review
- Full review pipeline → invoke /autoplan
- Bugs/errors → invoke /investigate
- QA/testing site behavior → invoke /qa or /qa-only
- Code review/diff check → invoke /review
- Visual polish → invoke /design-review
- Ship/deploy/PR → invoke /ship or /land-and-deploy
- Save progress → invoke /context-save
- Resume context → invoke /context-restore
- Author a backlog-ready spec/issue → invoke /spec
