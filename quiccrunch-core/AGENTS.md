# QuicCrunch Agent Task Documentation

This document explains the task documentation format used in the `tasks/` directory for QuicCrunch development and establishes critical rules for AI agent behavior.

## CRITICAL AGENT BEHAVIOR RULES

### Data Integrity and Honesty

**NEVER FABRICATE DATA**: Agents must never make up or fake:
- Test results or test outputs
- Shell command outputs
- Benchmark numbers or performance metrics
- Error messages or compilation results
- File contents or directory listings
- API responses or network data

**If real data is not available**:
- Explicitly ask the user to provide the needed information
- Clearly state what data is missing and why it's required
- Do not proceed with placeholder or assumed values
- Use phrases like "I need to see the actual output of..." or "Please run X command and share the results"

### Agent Delegation Requirements

**ALWAYS DELEGATE TO SPECIALIZED AGENTS**: When multiple agents are available, you must:
- Check available specialized agents before attempting work directly
- Delegate tasks that match an agent's specific expertise area
- Use the Agent tool for complex or domain-specific work
- Only handle simple, generic tasks directly

**Specialization Areas**:
- Code analysis and refactoring
- Performance optimization
- Testing and validation
- Documentation generation
- Architecture design

### Git-Based Experimentation Workflow

**MANDATORY GIT WORKFLOW**: All code changes must follow this pattern:

1. **Commit Current State**: Always commit working code before experimenting
   ```bash
   git add -A
   git commit -m "Working state before [experiment description]"
   ```

2. **Experiment Safely**: Make experimental changes with confidence
   - Try new approaches
   - Test different implementations
   - Iterate on solutions

3. **Validate or Rollback**: After each experiment
   - If successful: commit the changes with descriptive message
   - If failed: rollback using `git reset --hard HEAD~1` or `git checkout -- .`
   - Never leave broken code uncommitted

4. **Document Results**: Update task files with findings from experiments

### Resource Requirements

**ASK FOR MISSING RESOURCES**: When you need tools, libraries, or information not provided:
- **Explicitly request** what you need from the user
- **Never assume** availability of external libraries or tools
- **Check existing codebase** first (package.json, Cargo.toml, etc.)
- **Request additions** to flake.nix for new development tools
- **State limitations** clearly when resources are unavailable

**Examples of proper requests**:
- "I need the tQUIC 1.6.0 API documentation to implement this correctly"
- "This requires adding [library] to Cargo.toml - shall I proceed?"
- "I need to see the actual compiler output to diagnose these errors"

### Code Quality Mandates

**NO PLACEHOLDER CODE**: Eliminate all non-production code patterns:
- ❌ `testing_this`, `new_that`, `temp_method`
- ❌ `// TODO: implement this`
- ❌ `unimplemented!()` in production paths
- ✅ Use meaningful, descriptive names
- ✅ Implement complete functionality
- ✅ Add proper error handling

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

### Session Resumption
When resuming work:

1. Read the relevant task file(s) to understand current status
2. Check dependencies are completed
3. **Commit current working state** before making any changes
4. Update the task status as work progresses
5. Document any new insights or changes in the Notes section
6. Update related task files if dependencies change
7. **Delegate to specialized agents** for complex tasks

### Agent Delegation Strategy

**MANDATORY**: Before attempting any significant work, check for specialized agents:

1. **Code Analysis**: Use specialized agents for refactoring, optimization, debugging
2. **Architecture**: Delegate design decisions and system architecture work
3. **Testing**: Use testing specialists for comprehensive test implementation
4. **Performance**: Delegate benchmarking and optimization to performance agents
5. **Documentation**: Use documentation agents for comprehensive docs

**Only handle directly**:
- Simple file operations
- Basic git commands
- Environment setup
- Tool execution

### Development Environment Rules

#### NixOS/Flake Requirements
- **Environment Check**: Always verify `IN_NIX_SHELL` before running commands
- **Dev Shell**: If not in shell, use `nix develop -c -- <command>` and warn user to start devShell
- **Building**: Use `nix build` for compilation, `nix checks` for testing
- **Dependencies**: Add new tools to `flake.nix` devShell, not global installation
- **Resource Requests**: Always ask user for missing tools/libraries rather than assuming availability

#### Git & File Management
- **No Binaries**: Never commit compiled artifacts - maintain `.gitignore`
- **Test Scripts**: Only commit test scripts that are part of official testing framework
- **Commits**: Only commit when explicitly requested by user
- **File Tracking**: Always track process PIDs when starting background services (no `pkill -f`)
- **Git Workflow**: MANDATORY - Always commit working state before experimenting, rollback failed experiments
- **Experimentation**: Use git to safely try different approaches without losing working code

#### Code Quality Standards
- **Testing**: Always run tests - never disable or skip testing phases
- **Linting**: Run `nix develop -c -- cargo clippy` and `cargo fmt` before completion
- **Documentation**: Update relevant documentation in `tasks/` and `AGENTS.md`
- **Error Handling**: Address ALL compilation errors - no partial solutions
- **No Placeholders**: Eliminate all placeholder code like "testing_this", "new_that" - use meaningful names
- **Complete Implementations**: Never leave unimplemented!() in production code paths
- **Data Integrity**: NEVER fabricate test results, outputs, or any data - ask user for real information

#### QuicCrunch-Specific Rules

##### tQUIC Integration Constraints
- **Thread Safety**: Remember tQUIC v1.6.0 uses `Rc<RefCell<T>>` (not Send)
- **Architecture**: Must use multi-threaded worker pool pattern, not direct async integration
- **API Compatibility**: Always check tQUIC 1.6.0 API docs before making calls
- **Socket Sharing**: Use SO_REUSEPORT for worker thread socket distribution

##### Performance Requirements
- **Benchmarking**: All transport changes must include benchmark validation
- **Memory**: Monitor allocation patterns with connection pools
- **Profiling**: Use built-in profiling tools in `benches/` directory
- **Metrics**: Update connection pool hit rates and worker thread utilization

##### Current Compilation Issues (58+ Errors)
- **Root Cause**: tQUIC v1.6.0 uses `Rc<RefCell<T>>` making it incompatible with tokio multi-threading (we've always used tQUIC v1.6.0)
- **Key Files**: `async_bridge.rs`, `worker.rs`, `stream_splitter.rs`, `transport/mod.rs`
- **Critical Errors**:
  - API mismatches: `TlsConfig::default()`, `Endpoint::with_configs()` method calls incorrect
  - Field mismatches: `local_addr` should be `bind_addr` in WorkerConfig  
  - Send trait violations: tQUIC Endpoint cannot be shared between threads
  - Type mismatches: TquicStream vs QuicStream, TquicConnection vs QuicStream
- **Required Approach**: Complete multi-threaded worker pool implementation to isolate tQUIC in dedicated threads

##### Code Organization
- **Transport Layer**: Keep tQUIC-specific code in `src/transport/`
- **Async Bridge**: Use `async_bridge.rs` for tokio ↔ tQUIC integration
- **Worker Management**: Centralize worker logic in `worker_pool.rs` and `worker.rs`
- **Stream Handling**: Implement AsyncRead/AsyncWrite in `tquic_stream.rs`

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