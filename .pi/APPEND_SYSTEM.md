# ADP Workspace — Engram Integration

This workspace uses **engram** for persistent memory across sessions. Every agent working here must follow these rules.

## Purpose

You are an agent on the ADP (Agentic Development Platform) project. Your work spans three repos: `engram` (AI memory layer), `agentic-repos` (Git server + Skylet plugins), and `adp` (Svelte frontend). Everything you learn, decide, or discover must be stored in engram so the next agent — or the next session — can pick up where you left off.

## Engram CLI

The `engram` CLI is in PATH. All commands run against this workspace's `.engram/` directory.

## Workflow

### 1. Session lifecycle

Every session starts and ends with engram:

```bash
# Start
engram session start --name "<role>-<goal>"

# End (always generate summary for handoff)
engram session end --id <SESSION_ID> --generate-summary
```

### 2. Task management

All work is tracked as tasks. Grab the next one or create one:

```bash
engram next                                              # highest-priority pending task
engram task create --title "Title" --priority high --tags "tag1,tag2"
engram task update <UUID> --status in_progress
engram task update <UUID> --status done --outcome "result"
```

### 3. Context — store findings immediately

When you discover something — an error, a pattern, a file location, a config detail — write it as context linked to your task:

```bash
engram context create -t "Context: what you found" --content "Details..." --relevance high
engram relationship create --source-id <TASK_UUID> --source-type task \
  --target-id <CONTEXT_UUID> --target-type context \
  --relationship-type references --agent default
```

**Do not batch findings.** Write each one as you find it.

### 4. Reasoning — record every decision

When you choose an approach, pick a library, decide on architecture — record why:

```bash
engram reasoning create --title "Why we chose X" --task-id <TASK_UUID>
engram reasoning conclude <UUID> --conclusion "Final answer" --confidence 0.85
```

### 5. Lessons — record every mistake

When something goes wrong, record it so no agent repeats it:

```bash
engram lesson create -t "Lesson: short title" \
  -m "what went wrong" -c "the fix" -p "prevention rule" \
  -d "domain" -k "category" -s "severity" --tags "tags"
```

### 6. Knowledge — store durable facts

Project facts, patterns, rules, conventions:

```bash
engram knowledge create -t "Title" -k fact -c "Content" --tags "tags" --confidence 0.9
```

Types: `fact`, `rule`, `concept`, `pattern`, `procedure`, `heuristic`, `skill`, `technique`.

### 7. Relationships — link everything

No orphaned entities. After every create, link it:

```bash
engram relationship create \
  --source-id <UUID> --source-type <type> \
  --target-id <UUID> --target-type <type> \
  --relationship-type references --agent default
```

Types: `references`, `belongs_to`, `depends_on`, `about`, `relates_to`, `produced`, `blocked_by`.

### 8. Search — check before assuming

Before starting work, search for prior context:

```bash
engram ask query "keyword"                # single query string, not multiple words
engram ask query "oauth" --knowledge-type rule  # filter by type
engram relationship connected --entity-id <UUID>
```

## Invariants

- **Never leave entities orphaned.** Always link after creation.
- **One fact per knowledge entry.** Atomic, not mega-blobs.
- **Every decision gets a reasoning entry** with confidence scores.
- **Every mistake gets a lesson.** No repeats.
- **All commits reference a task UUID:** `[<UUID>]` in the commit message.
- **Context is written immediately**, not batched at the end.
- **Sessions always end with `--generate-summary`** for handoff.
- **All deletes are soft.** Entities persist forever.
- **`engram ask query` takes a single query string.** Not multiple positional args.
- **`engram relationship create` requires `--agent default`.**
