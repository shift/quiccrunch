# QuicCrunch Agent Task Documentation

This document explains the task documentation format used in the `tasks/` directory for QuicCrunch development.

## Purpose

The `tasks/` directory contains detailed documentation for each major development task, allowing AI agents to:
- Resume work from previous sessions with full context
- Understand the complete scope and requirements of each task
- Track dependencies between tasks
- Maintain consistency across implementation phases

## Task File Format

Each task is documented in `tasks/<task-id>.md` with the following structure:

### Required Sections

1. **Task Overview** - Brief description and goals
2. **Current Status** - What's been done, what's in progress, what's remaining
3. **Context** - Background information, related issues, architectural decisions
4. **Technical Requirements** - Specific implementation details and constraints
5. **Dependencies** - Other tasks that must be completed first
6. **Implementation Plan** - Step-by-step approach
7. **Files to Modify** - Specific files that will be changed
8. **Testing Strategy** - How to verify the implementation works
9. **Notes** - Additional considerations, gotchas, or insights

### Naming Convention

- Task IDs use snake_case and should be descriptive: `tquic_worker_pool`, `socket_reuse_implementation`
- File names match the task IDs: `tasks/tquic_worker_pool.md`

### Status Tracking

Each task should maintain its status:
- `pending` - Not started
- `in_progress` - Currently being worked on
- `completed` - Finished and tested
- `blocked` - Waiting on dependencies or external factors

## Cross-Task Context

### Architecture Context

For the tQUIC integration rewrite, the key architectural decision is:

**Multi-threaded Adapter Pattern**: Use a configurable number of tQUIC worker threads (each with mio::Poll + tQUIC::Endpoint) that share sockets via SO_REUSEPORT, with a tokio async adapter layer on top for ergonomic async/await APIs.

This solves the fundamental incompatibility between tQUIC v1.6.0's `Rc<RefCell<T>>` usage (not Send) and tokio's multi-threaded async runtime.

### Key Components

1. **TquicWorkerPool** - Manages N worker threads and load balancing
2. **TquicWorker** - Individual worker thread with mio event loop + tQUIC endpoint  
3. **TquicAdapter** - Async tokio interface that delegates to workers via channels
4. **Socket Management** - SO_REUSEPORT for efficient socket sharing

### Current Project State

- **tQUIC Version**: 1.6.0 (confirmed in Cargo.toml)
- **Major Issue**: 58+ compilation errors due to API incompatibilities
- **Root Cause**: Thread safety violations (Rc<RefCell<T>> not Send)
- **Solution**: Multi-threaded adapter pattern instead of direct async integration

## Agent Instructions

When resuming work:

1. Read the relevant task file(s) to understand current status
2. Check dependencies are completed
3. Update the task status as work progresses
4. Document any new insights or changes in the Notes section
5. Update related task files if dependencies change

## Example Usage

```bash
# Start working on a task
cat tasks/tquic_worker_pool.md

# Update task status
# Edit the task file to reflect current progress

# Check what other tasks are ready
grep -l "pending" tasks/*.md | grep -v blocked
```

This documentation system ensures continuity across AI agent sessions and maintains project momentum.