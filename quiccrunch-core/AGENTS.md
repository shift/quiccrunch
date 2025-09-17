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

## General Agent Delegation Protocol

The @general agent acts as a supervisor/orchestrator that ensures specialized agents complete work successfully rather than doing work itself. This section defines mandatory requirements for delegation monitoring and error handling.

### Pre-Delegation Checks

**ALWAYS DELEGATE FIRST**: Before attempting any significant work:

1. **Agent Availability Check**: Query available specialized agents using appropriate tools
2. **Task-Agent Matching**: Identify which agents match the specific task requirements:
   - Code analysis/refactoring → Code specialist agents
   - Performance optimization → Performance specialist agents  
   - Testing implementation → Testing specialist agents
   - Architecture design → Architecture specialist agents
   - Documentation → Documentation specialist agents
3. **Capability Assessment**: Verify the chosen agent has the required capabilities for the task
4. **Context Preparation**: Prepare complete context including:
   - Current task status from `tasks/*.md` files
   - Relevant codebase information
   - Previous attempt results (if any)
   - Specific requirements and constraints

### Delegation Execution

**MANDATORY MONITORING**: During agent execution:

1. **Error Detection**: Monitor for:
   - Agent failure or timeout messages
   - Context limit exceeded errors
   - Incomplete responses or partial results
   - Tool execution failures
   - Compilation or test failures

2. **Context Tracking**: Maintain detailed records of:
   - Which agent was delegated to
   - What specific task was assigned
   - Full context provided to the agent
   - Agent response and completion status
   - Any errors or issues encountered

3. **Progress Monitoring**: Verify agent is making meaningful progress:
   - Not stuck in loops or repeated attempts
   - Actually modifying files when required
   - Following project conventions and requirements
   - Addressing all aspects of the delegated task

### Post-Delegation Validation

**COMPLETION VERIFICATION**: After agent delegation:

1. **Result Quality Check**: Verify the agent actually completed the work:
   - Files were modified as required
   - Code compiles successfully (run compilation checks)
   - Tests pass (run relevant test suites)
   - Requirements from task documentation are met

2. **Integration Testing**: Ensure delegated work integrates properly:
   - No breaking changes to existing functionality
   - Follows project coding standards and conventions
   - Maintains compatibility with existing APIs
   - Passes linting and formatting checks

3. **Documentation Updates**: Update task files with delegation results:
   - Mark completed portions as done
   - Document any changes in approach or requirements
   - Note any issues discovered during implementation
   - Update dependencies if other tasks are affected

### Error Escalation Procedures

**SYSTEMATIC ERROR HANDLING**: When delegation fails:

1. **First Failure - Retry with Context**:
   - Include full error details from previous attempt
   - Provide additional context or clarification
   - Try the same specialist agent once more
   - Set shorter timeout/context limits if appropriate

2. **Second Failure - Alternative Agent**:
   - Switch to a different specialist agent with similar capabilities
   - Provide complete context from both previous attempts
   - Include lessons learned and specific error patterns
   - Request explicit confirmation of task understanding

3. **Third Failure - Model Escalation**:
   - Switch to a more capable model for the specialist agent
   - Provide comprehensive failure analysis
   - Break down the task into smaller, more specific subtasks
   - Consider requesting human intervention if critical path

4. **Persistent Failures - Task Decomposition**:
   - Break the original task into smaller, independent subtasks
   - Delegate each subtask to appropriate specialists
   - Sequence the subtasks to build up to the full solution
   - Document the decomposition approach for future reference

### Documentation Requirements

**MANDATORY UPDATES**: For every delegation:

1. **Task File Updates**: Update relevant `tasks/*.md` files with:
   - Which agents were used for what portions
   - Success/failure status of each delegation
   - Key insights or challenges discovered
   - Changes to implementation approach or requirements

2. **Delegation History**: Maintain a record including:
   - Timestamp and agent used
   - Task assigned and context provided
   - Result status (success/failure/partial)
   - Error details if applicable
   - Resolution approach if errors occurred

3. **Project Status**: Update project-wide status:
   - Dependencies between tasks if changed
   - Overall project timeline if affected
   - Known issues or blockers discovered
   - Recommendations for future work

### Emergency Procedures

**CRITICAL FAILURES**: If all delegation attempts fail:

1. **Immediate Actions**:
   - Commit any working code to prevent loss
   - Document the exact failure sequence and errors
   - Mark the task as "blocked" with detailed explanation
   - Identify if this blocks other dependent tasks

2. **Escalation**:
   - Report to user with complete failure analysis
   - Recommend specific resources or expertise needed
   - Suggest alternative approaches or task decomposition
   - Provide clear next steps for resolution

This protocol ensures the @general agent effectively coordinates specialist agents while maintaining project momentum and code quality.

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