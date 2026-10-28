# Briefs

The orchestrator fills in the names in angle brackets and starts each brief as a background subagent. The names:

- `<repo>`: the main checkout's absolute path, with forward slashes. `<base>`: the base branch.
- `<requirement>`: the requirement branch, checked out in `<integration>`, which is `<repo>/.claude/worktrees/<requirement>`.
- `<plan>`: the plan document, relative to the repository root.
- `<node>` and `<title>`: the node's id and title. `<branch>` is `<requirement>-<node in lower case>`, and `<worktree>` is `<repo>/.claude/worktrees/<branch>`. Never `<requirement>/<node>`: git cannot keep that beside the branch `<requirement>`.

Each brief ends with the report its subagent gives as its last message.

## Developer

```text
You develop node <node> ("<title>") of the repository at <repo>, plan <plan>, requirement branch <requirement>.

Set up your worktree from <requirement> as it is now:
  git -C <repo> worktree add -b <branch> <worktree> <requirement>
If <branch> already exists from an earlier attempt, keep working in its worktree. Work only inside <worktree>.

Read AGENTS.md there first; its rules bind you, the parallel development section and the locks on exclusive resources included. Lock under the holder name <node>, and release every lock you hold before you report, whatever the outcome.

Build what the node's section of <plan> asks for, within the plan's scope and architecture and the ADRs it names. Commit as you go, following the repository's commit rules. Before you report ready, rebase on the latest <requirement> and meet every item of the plan's done-when list, for every node and for this node.

<Each node merged so far, a line each: "- <id> <title>: what it delivers". Or: "No node has been merged yet.">
<What earlier nodes changed in the plan that this node must know.>
<On a second attempt: what the first one left in the worktree and why it stopped.>

When the plan cannot work as written, change it (the plan document, and a new ADR for a changed decision), carry on, and list each change in your report. Nobody is available to answer questions: decide, and report what you decided.

End with this report:
  Status: ready, only when the whole done-when list is met; otherwise failed
  Summary: what the node now delivers, or what is missing
  Not verified: what could not be tried in the running program, plainly
  Plan changes: each change, a paragraph each: what the plan said, why it could not work, what was done instead; or "none"
```

## Reviewer

```text
Review node <node> ("<title>") of the repository at <repo>, plan <plan>: branch <branch>, checked out in <worktree>.

Use the code-review skill, from the fixed point `git merge-base <requirement> <branch>`, with the node's section of <plan> and the plan's common done-when list as the spec. Without the skill, review by hand on the same two axes: the repository's standards (AGENTS.md and the code around the change), and the spec.

Check that the done-when list is met in fact: run the build and the tests; the record of the run in the program exists and says what was not verified; the node's plan section lists what it added.

Change nothing. Matters of taste are not blocking.

The developer's report:
<the developer's or the last fixer's report>

End with this report:
  Verdict: pass when nothing must change before merging; otherwise changes
  Issues: each blocking issue, one concrete item each: the file, what is wrong, what is expected; or "none"
```

## Fixer

```text
Node <node> ("<title>") of the repository at <repo>, plan <plan>, needs changes after review. Work in its worktree <worktree>, on <branch>, under AGENTS.md: lock as holder <node>, and release every lock before you report.

Issues:
<the reviewer's issues>

Fix each and commit. Rebase on the latest <requirement>, and make sure the plan's done-when list still holds: rerun what a change could affect, in the running program too when the change touches its behavior.

End with the developer's report; its plan changes are only the ones made now.
```

## Merger

```text
Merge node <node> ("<title>") into <requirement>. <requirement> is checked out in <integration>; work there and in the node's worktree <worktree>, nowhere else.

1. If <base> has commits <requirement> lacks (git -C <integration> rev-list --count <requirement>..<base>), merge <base> into <requirement> first with --no-ff, resolve conflicts, and run the build and tests.
2. In the node's worktree, rebase <branch> on <requirement>. Resolve conflicts keeping both sides' intent, then run the build and tests.
3. In <integration>: git merge --no-ff <branch>, with the summary line "Merge <node>: <title>". Run the build and tests on the result.
4. git -C <integration> push origin <requirement>
5. git -C <repo> worktree remove <worktree>, then git -C <integration> branch -d <branch>. Run branch -d from <integration>: the main checkout is on <base>, which does not contain <branch>, so git refuses there.

If a step fails in a way you cannot resolve, leave <requirement> as it was before step 3.

End with this report:
  Merged: yes or no, and why not
  Pushed: yes or no
  Notes: what happened, base merges and conflicts resolved included
```
