---
name: dag-dev
description: Settle a requirement with the user, cut it into a DAG of nodes, and develop the nodes in parallel worktrees merged into one requirement branch.
disable-model-invocation: true
---

# DAG development

The user is present for steps 1 and 2. From step 3 on, nobody stops to ask: decide and carry on, and tell the user each decision as soon as you know it. They judge it while the work goes on; when they redirect, follow.

## 1. Settle the requirement

Run the `grilling` skill on the requirement, checking every answer against the repository's own documents (README, GLOSSARY, ADRs, design and plan documents) and code. Ask about what to build, not how to schedule it: the DAG comes from the settled requirement in step 2, never during the questioning.

Done when the user confirms a written summary of the requirement in which every open question is answered.

## 2. Derive the DAG

Cut the settled requirement into nodes. A node is work that can be built and verified on its own and merged as one piece; an edge means "must be merged first". Work that fixes the shape later nodes build on (a refactor, shared types, a file layout others add to, a tool later nodes test with) comes first, in series; everything else fans out as wide as the edges allow. Give each node a short id (`F1`, `C3`) and title.

Draw the DAG for the user as a diagram and get their confirmation. Done when the user confirms the DAG.

## 3. Write the plan on a requirement branch

On a new branch named for the requirement, cut from the base branch, write and commit, following the repository's rules for documents and commits:

- an ADR for each decision that is hard to reverse, and the new terms in the glossary;
- a plan document: goal and scope, how the parts fit together, a done-when list every node must meet (build, tests, verification in the running program, records the repository keeps), and one section per node saying what it delivers and its own done-when;
- a pointer to the plan where the repository lists its documents.

The repository's AGENTS.md says how parallel development works there and which resources are exclusive; lock them with `scripts/lock.ps1`. Add that section if it is missing.

Push the branch. Then switch the main checkout back to the base branch and check the requirement branch out in its own worktree:

```bash
git worktree add .claude/worktrees/<requirement> <requirement>
```

## 4. Orchestrate the nodes

You are the orchestrator. Every stage of every node (developer, reviewer, fixer, merger) is a background subagent you start with the Agent tool, from its brief in [briefs.md](briefs.md). You are woken when each one finishes, so its report reaches you, and through you the user, as soon as it exists. The Workflow tool would run to the end without waking you, and nobody would hear of a plan change until every node is done.

Keep a table of the nodes: id, title, deps, stage, review round, worktree. Start a node's developer as soon as every node it depends on is merged, and every ready node at once: there is no limit.

When a subagent finishes, act on its report before anything else:

- **Developer or fixer reports ready.** Tell the user at once each plan change, as it described it, and what it could not verify in the running program. Weigh each change against the nodes still to come, and add what a later node must know to that node's brief. Then start a reviewer.
- **Reviewer asks for changes.** Start a fixer with the issues. When a third round still asks for changes, tell the user what is left and decide whether to go on.
- **Reviewer passes.** The node waits to merge. One merger runs at a time; start the next as soon as none is running.
- **Merger merged.** Check the requirement branch's log, then start every node that is now ready.
- **A stage failed, or ended without a report.** Look at what it left in the node's worktree and start it once more with that in the brief. If it fails again, tell the user, run `scripts/lock.ps1 -ReleaseHolder <node>`, keep the worktree, and leave the nodes that depend on it waiting.

While nodes run, the main checkout and the requirement worktree belong to the mergers, and the locked resources to the nodes that hold them. When the user asks how it is going, answer from the table.

Done when every node is merged or has failed twice, and no subagent is running.

## 5. Report

Tell the user, node by node: merged or failed and why, and what was not verified in the running program; for a failed node, name its worktree. Then the plan changes, a line each, since each was told in full when it came. Merging the requirement branch into the base branch is the user's call.
