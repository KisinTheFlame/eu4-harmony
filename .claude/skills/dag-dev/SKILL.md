---
name: dag-dev
description: Settle a requirement with the user, cut it into a DAG of nodes, and develop the nodes in parallel worktrees merged into one requirement branch.
disable-model-invocation: true
---

# DAG development

The user is present for steps 1 and 2 only. From step 3 on, nobody answers questions: decide, and report the decisions at the end.

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

## 4. Run the workflow

Start the Workflow tool with `scriptPath` set to `workflow.js` in this skill's folder inside the requirement worktree, and `args`:

```json
{
  "repo": "<absolute path of the main checkout, forward slashes>",
  "base": "<base branch>",
  "requirement": "<requirement branch>",
  "integration": "<repo>/.claude/worktrees/<requirement>",
  "plan": "<plan document, relative to the repository root>",
  "push": true,
  "nodes": [{ "id": "F1", "title": "Capability layer", "deps": [] }]
}
```

Each node is developed in its own worktree, reviewed until it passes, and merged into the requirement branch one at a time; a node whose dependency failed is skipped. While it runs, leave the main checkout and the requirement worktree alone.

## 5. Report

When the workflow returns, tell the user, node by node: merged, failed or skipped, and why; every plan change a node made, as it described it; what was not verified in the running program. For a failed node, its worktree is kept for inspection: name it. Merging the requirement branch into the base branch is the user's call.
