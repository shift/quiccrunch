---
name: setup-pi-engram-extensions
description: Full workspace bootstrap for the pi engram extensions suite. Checks prerequisites (pi, node, engram CLI), verifies all extensions are present and have their npm dependencies installed, then runs the model-failover setup to confirm subscription status and configure failover.json and model-routing.json. Use this when setting up a new machine or handing off to a new agent or developer.
---

# Setup Pi Engram Extensions

Bootstraps the full suite of pi extensions used in this workspace. Covers:
- Prerequisite checks (tools, extensions, configs)
- npm dependency installation for package-style extensions
- Model failover configuration (subscription check + failover.json + model-routing.json)
- Engram workspace initialisation

> **Per-turn write rule:** This skill runs scripts and installs packages. After every script run or install step, store findings with `engram_context_create` + `engram_relationship_create`. Create a task at the start to anchor all context to it.

Read [extensions-overview.md](references/extensions-overview.md) for a description of each extension.

---

## Step 1 — Check prerequisites

Run from the workspace root:

```bash
bash .pi/skills/setup-pi-engram-extensions/scripts/check-prerequisites.sh
```

This checks:
- `pi`, `node`, `engram` CLI are in PATH
- All expected extension directories exist with their `index.ts`
- `npm install` has been run (node_modules present)
- `.pi/failover.json` and `.pi/model-routing.json` exist and are valid JSON
- `~/.pi/agent/auth.json` OAuth providers and key env vars

**If there are failures (✗):** fix them before continuing. Warnings (⚠) are non-blocking.

**After running the script, store the result:**
```
engram_context_create
  title="Prerequisites check: <pass/fail count>"
  content="<paste summary line from script output>"
  relevance="medium"

engram_relationship_create  source_id=<context UUID>  source_type="context"
                            target_id=<task UUID>     target_type="task"
                            relationship_type="relates_to"  agent="<name>"
```

Common fixes:

| Failure | Fix |
|---|---|
| `engram not found` | Install engram: follow the engram repo README |
| `node_modules missing` | `bash .pi/skills/setup-pi-engram-extensions/scripts/install-extension-deps.sh` |
| `failover.json not found` | Continue to Step 3 — it will be created |
| `.engram/ not found` | Run `engram init` in the workspace root |

---

## Step 2 — Install extension dependencies

If any extension reported missing `node_modules`, run:

```bash
bash .pi/skills/setup-pi-engram-extensions/scripts/install-extension-deps.sh
```

Then re-run the prerequisites check to confirm everything is green.

**After installing, store the result:**
```
engram_context_create
  title="Extension deps installed"
  content="All package-style extensions have node_modules installed."
  relevance="low"

engram_relationship_create  source_id=<context UUID>  source_type="context"
                            target_id=<task UUID>     target_type="task"
                            relationship_type="relates_to"  agent="<name>"
```

---

## Step 3 — Configure model failover

This is the most important step. Load the `setup-model-failover` skill and follow its full protocol:

```
/skill:setup-model-failover
```

That skill will:
1. Run `check-providers.sh` to detect configured providers
2. Ask the user to confirm which subscriptions are still active
3. Update `.pi/failover.json` and `.pi/model-routing.json` accordingly

**Do not skip this step** — the engram-agents and engram-orchestrator extensions use these configs to pick the right model for every dispatched subagent.

---

## Step 4 — Verify extensions load

Ask the user to run `/reload` in pi (or restart pi) and then run:

```
/agent-list
```

If the engram-agents extension loaded correctly, this shows the 171 available agent personas.

Also check:
```
/failover-status
```

If model-failover loaded correctly, this shows the current model and the priority list.

And:
```
/engram-auto-status
```

If engram-orchestrator loaded, this confirms the auto mode extension is active.

---

## Step 5 — Summary and handoff

Report to the user:
- Which extensions loaded successfully
- Current active model and failover model count
- Engram workspace status (tasks, sessions)
- Any remaining warnings or next steps

If this is a handoff to a new agent or developer, also tell them:
- The skill to re-run if subscriptions change: `/skill:setup-model-failover`
- The skill to re-run for full setup on a new machine: `/skill:setup-pi-engram-extensions`
- Where configs live: `.pi/failover.json`, `.pi/model-routing.json`, `.pi/extensions/`
- Prior decisions recorded in engram: `engram ask query "model routing failover extensions"`
