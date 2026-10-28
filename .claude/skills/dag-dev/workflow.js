export const meta = {
  name: 'dag-dev',
  description: 'Develop the nodes of a planned DAG in parallel worktrees, review each, and merge them one at a time into the requirement branch',
  whenToUse: 'Started by the dag-dev skill once the plan document, the requirement branch and its worktree exist',
  phases: [
    { title: 'Develop', detail: 'one agent per node, in its own worktree' },
    { title: 'Review', detail: 'review against the plan, fix until it passes' },
    { title: 'Merge', detail: 'one node at a time into the requirement branch' },
  ],
}

// args: { repo, base, requirement, integration, plan, push, maxReviewRounds?, nodes: [{ id, title, deps }] }
const A = args
const MAX_REVIEW_ROUNDS = A.maxReviewRounds ?? 3
const byId = Object.fromEntries(A.nodes.map(n => [n.id, n]))
for (const n of A.nodes) {
  for (const d of n.deps) if (!byId[d]) throw new Error(`node ${n.id} depends on unknown node ${d}`)
}

const slug = id => id.toLowerCase()
// Not `<requirement>/<node>`: git cannot keep that beside the branch `<requirement>`.
const branchOf = n => `${A.requirement}-${slug(n.id)}`
const worktreeOf = n => `${A.repo}/.claude/worktrees/${A.requirement}-${slug(n.id)}`

const DEV_SCHEMA = {
  type: 'object',
  properties: {
    status: { type: 'string', enum: ['ready', 'failed'] },
    summary: { type: 'string', description: 'What the node now delivers, or what is missing when failed' },
    notVerified: { type: 'string', description: 'What could not be tried in the running program, plainly' },
    planChanges: {
      type: 'array',
      items: { type: 'string' },
      description: 'Each change to the plan: what it said, why it could not work, what was done instead',
    },
  },
  required: ['status', 'summary', 'notVerified', 'planChanges'],
}

const REVIEW_SCHEMA = {
  type: 'object',
  properties: {
    verdict: { type: 'string', enum: ['pass', 'changes'] },
    issues: { type: 'array', items: { type: 'string' }, description: 'Blocking issues: file, what is wrong, what is expected' },
  },
  required: ['verdict', 'issues'],
}

const MERGE_SCHEMA = {
  type: 'object',
  properties: {
    merged: { type: 'boolean' },
    pushed: { type: 'boolean' },
    notes: { type: 'string', description: 'What happened, including base merges and conflicts resolved' },
  },
  required: ['merged', 'pushed', 'notes'],
}

const where = n => `the repository at ${A.repo}, plan ${A.plan}, requirement branch ${A.requirement}`

function devPrompt(n, merged) {
  const before = merged.length
    ? `Nodes merged into ${A.requirement} before you started:\n${merged.map(r => `- ${r.id} ${r.title}: ${r.summary}`).join('\n')}`
    : `No node has been merged into ${A.requirement} yet.`
  return `You develop node ${n.id} ("${n.title}") of ${where(n)}.

Set up your worktree from ${A.requirement} as it is now:
  git -C ${A.repo} worktree add -b ${branchOf(n)} ${worktreeOf(n)} ${A.requirement}
If ${branchOf(n)} already exists from an earlier attempt, keep working in its worktree. Work only inside ${worktreeOf(n)}.

Read AGENTS.md there first; its rules bind you, the parallel development section and the locks on exclusive resources included. Lock under the holder name ${n.id}, and release every lock you hold before you report, whatever the outcome.

Build what the node's section of ${A.plan} asks for, within the plan's scope and architecture and the ADRs it names. Commit as you go, following the repository's commit rules. Before you report ready, rebase on the latest ${A.requirement} and meet every item of the plan's done-when list, for every node and for this node.

${before}

When the plan cannot work as written, change it (the plan document, and a new ADR for a changed decision), carry on, and list each change in planChanges. Nobody is available to answer questions: decide, and report what you decided.

Report status "ready" only when the whole done-when list is met; otherwise "failed", with what is missing in summary.`
}

function reviewPrompt(n, dev) {
  return `Review node ${n.id} ("${n.title}") of ${where(n)}: branch ${branchOf(n)}, checked out in ${worktreeOf(n)}.

Use the code-review skill if you have it, from the fixed point \`git merge-base ${A.requirement} ${branchOf(n)}\`, with the node's section of ${A.plan} and the plan's common done-when list as the spec. Without it, review by hand on the same two axes: the repository's standards (AGENTS.md and the code around the change), and the spec.

Check that the done-when list is met in fact: run the build and the tests; the in-program test record exists and says what was not verified; the node's plan section lists what it added.

Change nothing. Verdict "pass" when nothing must change before merging; otherwise "changes", each issue one concrete item: the file, what is wrong, what is expected. Matters of taste are not blocking.

The developer's report: ${JSON.stringify(dev)}`
}

function fixPrompt(n, review) {
  return `Node ${n.id} ("${n.title}") of ${where(n)} needs changes after review. Work in its worktree ${worktreeOf(n)}, on ${branchOf(n)}, under AGENTS.md (lock as holder ${n.id}, and release every lock before you report).

Issues:
${review.issues.map(i => `- ${i}`).join('\n')}

Fix each and commit. Rebase on the latest ${A.requirement}, and make sure the plan's done-when list still holds: rerun what a change could affect, in the running program too when the change touches its behavior. Report as the developer did; planChanges lists only changes made now.`
}

function mergePrompt(n) {
  return `Merge node ${n.id} ("${n.title}") into ${A.requirement}. ${A.requirement} is checked out in ${A.integration}; work there and in the node's worktree ${worktreeOf(n)}, nowhere else.

1. If ${A.base} has commits ${A.requirement} lacks (git -C ${A.integration} rev-list --count ${A.requirement}..${A.base}), merge ${A.base} into ${A.requirement} first with --no-ff, resolve conflicts, and run the build and tests.
2. In the node's worktree, rebase ${branchOf(n)} on ${A.requirement}. Resolve conflicts keeping both sides' intent, then run the build and tests.
3. In ${A.integration}: git merge --no-ff ${branchOf(n)}, with the summary line "Merge ${n.id}: ${n.title}". Run the build and tests on the result.
4. ${A.push ? `git push origin ${A.requirement}` : `Do not push.`}
5. git -C ${A.repo} worktree remove ${worktreeOf(n)}, then git -C ${A.repo} branch -d ${branchOf(n)}.

If a step fails in a way you cannot resolve, leave ${A.requirement} as it was before step 3 and report merged false with the reason.`
}

function releasePrompt(n) {
  return `Run, in ${A.repo}: powershell -NoProfile -File .claude/skills/dag-dev/scripts/lock.ps1 -ReleaseHolder ${n.id}
Report its output.`
}

// Merges run one at a time, in the order nodes become ready.
let mergeQueue = Promise.resolve()
function merge(n) {
  const run = mergeQueue.then(() => agent(mergePrompt(n), { label: `merge:${n.id}`, phase: 'Merge', schema: MERGE_SCHEMA }))
  mergeQueue = run.catch(() => null)
  return run
}

const mergedSoFar = []
const results = {}

async function develop(n) {
  const deps = await Promise.all(n.deps.map(result))
  const blocked = deps.filter(d => d.status !== 'merged').map(d => d.id)
  if (blocked.length) {
    log(`${n.id} skipped: ${blocked.join(', ')} not merged`)
    return { id: n.id, title: n.title, status: 'skipped', summary: `depends on ${blocked.join(', ')}, not merged`, planChanges: [] }
  }

  const planChanges = []
  let dev = await agent(devPrompt(n, [...mergedSoFar]), { label: `dev:${n.id}`, phase: 'Develop', schema: DEV_SCHEMA })
  if (dev) planChanges.push(...dev.planChanges)

  let review = null
  let round = 0
  while (dev && dev.status === 'ready' && round < MAX_REVIEW_ROUNDS) {
    round++
    review = await agent(reviewPrompt(n, dev), { label: `review:${n.id}#${round}`, phase: 'Review', schema: REVIEW_SCHEMA })
    if (!review || review.verdict === 'pass') break
    log(`${n.id} review ${round}: ${review.issues.length} issue(s)`)
    const fixed = await agent(fixPrompt(n, review), { label: `fix:${n.id}#${round}`, phase: 'Develop', schema: DEV_SCHEMA })
    if (fixed) planChanges.push(...fixed.planChanges)
    dev = fixed && { ...fixed, notVerified: fixed.notVerified || dev.notVerified }
  }

  const passed = dev && dev.status === 'ready' && review && review.verdict === 'pass'
  if (!passed) {
    await agent(releasePrompt(n), { label: `release:${n.id}`, phase: 'Develop', effort: 'low' })
    const why = !dev ? 'the developer stopped without a report'
      : dev.status !== 'ready' ? dev.summary
      : !review ? 'the reviewer stopped without a verdict'
      : `review still asks for changes after ${round} round(s): ${review.issues.join('; ')}`
    log(`${n.id} failed: ${why}`)
    return { id: n.id, title: n.title, status: 'failed', summary: why, notVerified: dev?.notVerified, planChanges, worktree: worktreeOf(n) }
  }

  const merged = await merge(n)
  if (!merged || !merged.merged) {
    const why = merged ? merged.notes : 'the merger stopped without a report'
    log(`${n.id} not merged: ${why}`)
    return { id: n.id, title: n.title, status: 'failed', summary: `not merged: ${why}`, notVerified: dev.notVerified, planChanges, worktree: worktreeOf(n) }
  }
  log(`${n.id} merged${merged.pushed ? ' and pushed' : ''}`)
  const done = { id: n.id, title: n.title, status: 'merged', summary: dev.summary, notVerified: dev.notVerified, planChanges, reviewRounds: round, mergeNotes: merged.notes }
  mergedSoFar.push(done)
  return done
}

function result(id) {
  results[id] ??= develop(byId[id]).catch(e => {
    log(`${id} failed: ${e}`)
    return { id, title: byId[id].title, status: 'failed', summary: String(e), planChanges: [] }
  })
  return results[id]
}

const all = await Promise.all(A.nodes.map(n => result(n.id)))
return { requirement: A.requirement, nodes: all }
