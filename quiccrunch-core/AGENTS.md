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

## Context Research and Implementation Protocol

This section establishes the separation of concerns between context research and implementation phases to maximize code quality and agent efficiency.

### Context Researcher Agent

**PRIMARY ROLE**: The Context Researcher Agent is responsible for gathering comprehensive implementation details before any code implementation begins. This agent serves as the foundation for all subsequent implementation work.

**Core Responsibilities**:

1. **Comprehensive Code Analysis**:
   - Examine all relevant source files and their dependencies
   - Analyze existing patterns, conventions, and architectural decisions
   - Map out integration points and API surface areas
   - Document current implementation approaches and styles

2. **Technical Specification Preparation**:
   - Research applicable APIs, libraries, and framework documentation
   - Identify specific interfaces, types, and method signatures required
   - Document error handling patterns and edge cases
   - Prepare detailed technical requirements with concrete examples

3. **Context Completeness Verification**:
   - Ensure 95% of implementation context is gathered upfront
   - Verify all dependencies, imports, and integration points are documented
   - Confirm understanding of business logic and requirements
   - Validate that implementation agents won't need to perform research

4. **Implementation Blueprint Creation**:
   - Provide specific file modifications with exact locations
   - Include precise type signatures and interface requirements
   - Document testing approaches and validation criteria
   - Prepare comprehensive context packages for implementation agents

**Context Package Requirements**:
- Complete technical specifications with API details
- Relevant code snippets and integration examples
- Clear acceptance criteria and validation methods
- Specific file paths and modification points
- Error handling requirements and edge cases
- Testing strategy and validation approach

### Mandatory Git Workflow

**SACRED REPOSITORY INTEGRITY**: All agents must follow strict git-based experimentation to protect repository state.

#### Pre-Experimentation Requirements

**ALWAYS COMMIT WORKING STATE FIRST**:
```bash
# Before ANY code changes or experiments
git add -A
git commit -m "Working state before [specific experiment/change description]"
```

**Verification Steps**:
1. Confirm current code compiles and tests pass
2. Verify no uncommitted changes exist
3. Create descriptive commit message indicating the upcoming experiment
4. Push to backup branch if working on critical features

#### Experimentation Safety Protocol

**SYSTEMATIC APPROACH**: Every code change follows this pattern:

1. **Safe Experimentation**: 
   - Make experimental changes with confidence
   - Try multiple approaches if needed
   - Iterate rapidly on solutions
   - Document findings during experimentation

2. **Immediate Validation**:
   - Test compilation after each significant change
   - Run relevant test suites continuously
   - Validate functionality against requirements
   - Check for integration issues

3. **Decision Point Protocol**:
   - **SUCCESS**: Commit with descriptive message explaining the solution
   - **FAILURE**: Immediate rollback using one of these methods:
     ```bash
     # Rollback to previous commit (if changes committed)
     git reset --hard HEAD~1
     
     # Discard all working changes (if uncommitted)  
     git checkout -- .
     
     # Rollback specific files
     git checkout HEAD -- path/to/file.rs
     ```

#### Code Quality Mandates

**ELIMINATE PLACEHOLDER PATTERNS**: All placeholder naming patterns are forbidden:

❌ **Prohibited Patterns**:
- `new_<anything>` (new_client, new_stream, new_worker)
- `testing_<anything>` (testing_handler, testing_endpoint)
- `temp_<anything>` (temp_connection, temp_buffer)
- `old_<anything>` (old_implementation, old_method)
- `_backup`, `_v2`, `_copy`, `_test` suffixes
- Generic names like `thing`, `stuff`, `data`, `handler`

✅ **Required Approach**:
- Use descriptive, domain-specific names from the start
- Follow existing project naming conventions
- Choose names that clearly indicate purpose and scope
- Examples: `tquic_worker_pool`, `connection_manager`, `stream_multiplexer`

**NO BROKEN CODE COMMITS**: 
- Never commit code that doesn't compile
- Never commit code with failing tests
- Never commit code with unresolved TODO items in production paths
- Always verify full compilation before committing

**REPOSITORY STATE PROTECTION**:
- Repository integrity is sacred - any agent damaging repo state is failing
- Always prefer rollback over attempting to fix broken experiments
- Use git branches for risky experiments
- Maintain clean git history with meaningful commit messages

### Implementation Agent Protocol

**CONTEXT-RICH IMPLEMENTATION**: Implementation agents receive comprehensive context and focus purely on coding excellence.

#### Pre-Implementation Context

**Complete Context Package**:
- Detailed technical specifications from Context Researcher
- All relevant code examples and patterns
- Specific API requirements and interface definitions
- Error handling approaches and edge cases
- Testing criteria and validation methods
- Integration points and dependency requirements

#### Implementation Focus Areas

**PURE CODING EXCELLENCE**:
1. **No Research During Implementation**:
   - All context pre-gathered by Context Researcher
   - No API documentation lookup during coding
   - No architectural exploration during implementation
   - Focus purely on translating specifications to code

2. **Quality Implementation Standards**:
   - Follow exact specifications provided in context
   - Implement comprehensive error handling
   - Use meaningful names that match domain terminology
   - Write self-documenting code with clear intent

3. **Validation Integration**:
   - Implement testing as specified in context package
   - Verify all acceptance criteria are met
   - Ensure integration with existing systems
   - Confirm no regressions in existing functionality

4. **Git Integration**:
   - Follow mandatory git workflow for all changes
   - Commit working states before major modifications
   - Rollback immediately on compilation failures
   - Maintain clean, documented git history

#### Success Criteria

**IMPLEMENTATION COMPLETION**:
- All specifications from context package implemented
- Code compiles successfully without warnings
- All tests pass including new and existing test suites
- Integration tests verify proper system integration
- Documentation updated if API changes were made
- No placeholder code or TODO items in production paths

**Quality Gates**:
- Follows project coding conventions and patterns
- Implements robust error handling for all edge cases
- Uses meaningful, descriptive naming throughout
- Maintains or improves system performance characteristics
- Preserves existing API compatibility unless explicitly changing

This protocol ensures maximum efficiency by separating context research from implementation, allowing each agent to focus on their specialized area while maintaining the highest code quality standards.

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

## Current Active Tasks

### Streaming RPC Implementation (P0 CRITICAL)
**Status**: In Progress  
**Priority**: P0 - Blocks production readiness  
**Created**: 2025-10-21  
**Tracking**: See `quiccrunch-core/todo.json` and opencode todo list

**Objective**: Complete streaming RPC implementation and make bidirectional_streaming.rs example work

**Atomic Tasks**:
1. **Implement streaming RPC server infrastructure** - Complete `RpcServer::handle_connection()` with streaming pattern support (server/client/bidirectional)
2. **Fix streaming RPC client methods** - Implement `server_streaming_call()`, `client_streaming_call()`, `bidirectional_streaming_call()` in RpcClient
3. **Integrate with tQUIC backend** - Ensure stream operations work end-to-end with tQUIC transport
4. **Update example code** - Refactor `examples/bidirectional_streaming.rs` to use RPC framework (RpcClient/RpcServer) instead of raw transport layer
5. **Comprehensive testing** - Validate all streaming patterns (unary, server streaming, client streaming, bidirectional) work correctly

**Files Involved**:
- `src/rpc.rs` - Core RPC server and client implementation
- `src/rpc/streaming.rs` - Streaming infrastructure (if exists)
- `src/transport/adapter.rs` - tQUIC transport integration
- `examples/bidirectional_streaming.rs` - Example code
- `tests/streaming_rpc_bdd.rs` - BDD test scenarios

**Success Criteria**:
- All streaming RPC patterns compile and run successfully
- `bidirectional_streaming.rs` example works end-to-end
- Tests pass for all streaming scenarios
- Documentation updated with streaming examples

---

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