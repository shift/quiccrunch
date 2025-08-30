# Context Management Workflow Documentation

## CRITICAL: Real-Time Context Update Protocol

**Context files MUST be updated after EVERY significant action.** This is not optional - it ensures context limit recovery with maximum 1 step behind current state.

### Mandatory Update Triggers:
1. **After each file edit** - Update Code Locations section
2. **After each test run** - Update Testing section with results
3. **After each decision** - Update Key Decisions with rationale
4. **After each git operation** - Update Implementation Notes
5. **Before any long operation** - Save current state
6. **When approaching context limits** - Ensure complete state capture

## Session Startup Protocol

### 1. Immediate Context Load
When starting ANY session:
```bash
# FIRST: Load active work context (no searching needed)
cat ai-agent-workflow/todo.md
# SECOND: Load specific context file for active task
cat ai-agent-workflow/context/<active-task-name>.md
```

### 2. Zero-Search Recovery
- Context file contains ALL information needed to continue
- No investigation or searching required
- Exact next step is documented
- Current progress state is captured
- Environment setup is documented

### 3. Environment Validation
```bash
# Check if this is a NixOS environment
if command -v nix >/dev/null 2>&1; then
  echo "NixOS environment detected"
  # Use nix develop if available
  nix develop -c -- <command>
else
  echo "Standard environment"
  # Run commands directly
  <command>
fi

# Run health checks and validation tests
# (Adapt these commands to your project's testing framework)
<test-command>
```

## Real-Time Context Maintenance

### After Each File Edit
**IMMEDIATELY update context file:**
```markdown
## Code Locations (Last updated: 13:25 CEST)
- `src/example.rs:45-60`: Added validate_input function (13:20 CEST)
- `tests/unit.rs:120`: Added test case for validation (13:22 CEST)

## Current Progress (Last updated: 13:25 CEST)
- ✅ Implement error handling in parser
- 🔄 Currently: Adding comprehensive test coverage
- ⏭️ Next: Run test suite and validate changes
```

**Then immediately commit:**
```bash
git add ai-agent-workflow/context/<task-name>.md src/example.rs tests/unit.rs
git commit -m "context: fix-parser-bug - added error handling + test coverage"
```

### After Each Test Run
**IMMEDIATELY update context file:**
```markdown
## Testing Progress (Last updated: 13:30 CEST)
- **13:25 CEST**: `<test-command>` - PASSED (3 tests)
  - All basic validation cases working
  - Edge case test_empty_input passed
- **13:28 CEST**: `<integration-test-command>` - FAILED
  - Error: connection timeout in setup
  - Need to fix: database connection string
- **13:30 CEST**: Identified issue: parser not thread-safe
- **Next**: Add mutex protection around shared state
```

**Then immediately commit:**
```bash
git add ai-agent-workflow/context/<task-name>.md
git commit -m "context: fix-parser-bug - documented test failures and next steps"
```

### After Each Decision
**IMMEDIATELY update context file:**
```markdown
## Key Decisions (Last updated: 14:35:00)
- **14:33:20**: Use Arc<Mutex<T>> for thread safety
  - Problem: Race conditions in concurrent parsing
  - Considered: RwLock (rejected - write-heavy workload)
  - Considered: Lock-free data structures (rejected - complexity)
  - Chosen: Mutex with minimal critical sections
  - Implementation plan: Wrap parser state in Arc<Mutex<>>
```

**Then immediately commit:**
```bash
git add ai-agent-workflow/context/<task-name>.md
git commit -m "context: fix-parser-bug - decided on Arc<Mutex> for thread safety"
```

### Before Context Limits
**IMMEDIATELY save state before approaching limits:**
```markdown
## Emergency Context Save (13:35 CEST)
- **Exact current task**: Adding thread safety to parser
- **Current file**: src/parser/mod.rs
- **Current line**: 145 (adding Mutex wrapper)
- **Compilation status**: Does not compile (missing imports)
- **Next 3 steps**:
  1. Add `use std::sync::{Arc, Mutex};`
  2. Wrap `ParserState` in `Arc<Mutex<ParserState>>`
  3. Update all access points to use `.lock().unwrap()`
- **Test status**: 2 tests failing due to race conditions
- **Branch status**: 3 uncommitted files with changes
```

## Implementation Workflow

### 1. Start Work
- Mark task as "In Progress" in todo.md **with timestamp**
- Update context file with session start time and plan
- Document initial approach and immediate next steps

### 2. During Implementation (Real-Time Updates)
**Update context file after EVERY action, then COMMIT immediately:**

#### File Edit Pattern:
```markdown
## Code Locations (Last updated: 13:25 CEST)
- `src/parser/mod.rs:156-170`: Added error handling (13:20 CEST)
- `src/parser/rust.rs:89`: Fixed symbol extraction bug (13:22 CEST)
- `tests/parser_test.rs:45`: Added test for error case (13:25 CEST)

## Current Progress (Last updated: 13:25 CEST)
- ✅ Implement error handling in parser
- 🔄 Currently: Adding comprehensive test coverage
- ⏭️ Next: Run test suite and validate changes
```

**Then immediately commit:**
```bash
git add ai-agent-workflow/context/<task-name>.md src/parser/mod.rs src/parser/rust.rs tests/parser_test.rs
git commit -m "context: fix-parser-bug - added error handling + test coverage"
```

#### Test Run Pattern:
```markdown
## Testing Log (Last updated: 13:30 CEST)
- **13:25 CEST**: `<test-command>` - PASSED (12 tests)
- **13:28 CEST**: `<integration-test-command>` - FAILED (2/8 tests)
  - FAIL: test_large_file_parsing - timeout after 30s
  - FAIL: test_concurrent_parsing - race condition
- **13:30 CEST**: Identified issue: parser not thread-safe
- **Next**: Add mutex protection around shared state
```

**Then immediately commit:**
```bash
git add ai-agent-workflow/context/<task-name>.md
git commit -m "context: fix-parser-bug - documented test failures and next steps"
```

#### Decision Pattern:
```markdown
## Key Decisions (Last updated: 13:35 CEST)
- **13:33 CEST**: Use Arc<Mutex<T>> for thread safety
  - Problem: Race conditions in concurrent parsing
  - Considered: RwLock (rejected - write-heavy workload)
  - Considered: Lock-free data structures (rejected - complexity)
  - Chosen: Mutex with minimal critical sections
  - Implementation plan: Wrap parser state in Arc<Mutex<>>
```

### 3. Context Limit Management
**When approaching context limits (before hitting them):**

1. **Save complete current state**:
```markdown
## Emergency Context Save (13:35 CEST)
- **Exact current task**: Adding thread safety to parser
- **Current file**: src/parser/mod.rs
- **Current line**: 145 (adding Mutex wrapper)
- **Compilation status**: Does not compile (missing imports)
- **Next 3 steps**:
  1. Add `use std::sync::{Arc, Mutex};`
  2. Wrap `ParserState` in `Arc<Mutex<ParserState>>`
  3. Update all access points to use `.lock().unwrap()`
- **Test status**: 2 tests failing due to race conditions
- **Branch status**: 3 uncommitted files with changes
```

2. **Immediately commit emergency state**:
```bash
git add .
git commit -m "context: emergency save - approaching context limits"
```

3. **Document recovery instructions**:
```markdown
## Recovery Instructions
1. Open file: `src/parser/mod.rs` at line 145
2. Add missing imports for Arc and Mutex
3. Complete the mutex wrapper implementation
4. Run: `<test-command>`
5. If tests pass, proceed to integration tests
6. If tests fail, check mutex lock scope issues
```

4. **Commit recovery instructions**:
```bash
git add ai-agent-workflow/context/<task-name>.md
git commit -m "context: emergency recovery instructions documented"
```

### 4. Session Handoff Protocol

#### Before Session End:
```markdown
## Session End State (13:40 CEST)
- **Completed**: Thread safety implementation in parser
- **Test Status**: All unit tests passing, 1 integration test failing
- **Current Issue**: Deadlock in concurrent parsing test
- **Next Session Start**: Debug deadlock in test_concurrent_parsing
- **Files Changed**:
  - src/parser/mod.rs (committed in abc123f)
  - tests/integration.rs (uncommitted - has debug prints)
- **Environment**: Development environment active, build cache warm
```

**MANDATORY**: Commit session end state:
```bash
git add ai-agent-workflow/context/<task-name>.md
git add tests/integration.rs  # Include any uncommitted work files
git commit -m "context: session end state - ready for handoff"
```

#### Next Session Recovery:
1. **Load context file** - contains complete state
2. **Check git log** - `git log --oneline -5` for recent context commits
3. **Verify environment** - Check if development environment needs reactivation
4. **Check file states** - `git status` to verify uncommitted changes
5. **Continue from exact step** - debug deadlock in test_concurrent_parsing

#### Git Integration Best Practices:
- **Commit frequency**: After every significant context update
- **Commit messages**: Use format `context: <task-name> - <action>`
- **Include all changes**: Context files + any related code modifications
- **Reference commits**: Always include commit hashes in context files
- **Clean commits**: Each context update should be a separate, focused commit

## Template Updates

### Updated Context Template
```markdown
# Context: [Task Name]

## Task Summary
[Brief description and link to task file]

## Current State (Last updated: HH:MM TIMEZONE)
- **Working on**: [Exact current action]
- **Next step**: [Immediate next action]
- **Files open**: [Currently editing]
- **Compilation status**: [Compiles/errors/warnings]

## Key Decisions (Updated in real-time)
- **HH:MM:SS**: [Decision]
  - Problem: [What needed solving]
  - Rationale: [Why this choice]
  - Alternatives: [What was considered]
  - Implementation: [How it's being done]

## Code Locations (Updated after each edit)
- `file:line-range`: [Description] (HH:MM:SS)

## Testing Log (Updated after each test run)
- **HH:MM:SS**: `command` - RESULT (details)

## Session Continuity (Updated continuously)
- **Current directory**: [pwd]
- **Active branch**: [git branch]
- **Uncommitted files**: [git status]
- **Next action**: [Exact step to take]
- **Expected outcome**: [What should happen]
- **If it fails**: [Debugging approach]

---
*Real-time context for: [Task Name]*
*Last updated: [timestamp]*
```

This ensures any AI agent can load a context file and continue from exactly where work left off, with zero investigation or searching required.

## Context Maintenance

### File Updates
- Update context files throughout implementation, not just at end
- Include specific line numbers for code references
- Document both successful and failed approaches
- Note any environment-specific considerations

### Cross-References
- Link related tasks and contexts
- Reference existing documentation
- Connect to relevant issues or external resources

### Version Control
- Include git commit hashes for significant changes
- Reference branch names and PR numbers when applicable
- Document any merge conflicts or resolution strategies

## Session Handoff

### Before Ending Session
1. Update all relevant context files
2. Mark current progress in task files
3. Update todo.md with current status
4. Document any immediate next steps
5. Note any session-specific environment setup

### For Next Session
1. Review todo.md for current priorities
2. Read context files for full background
3. Verify environment setup and dependencies
4. Run health checks and validation tests
5. Continue from documented next steps

## Quality Guidelines

### Context File Quality
- Be specific about decisions and rationale
- Include enough detail for complete reconstruction
- Document failed approaches to avoid repetition
- Maintain professional, technical tone

### Task File Quality
- Clear, measurable acceptance criteria
- Realistic implementation timelines
- Proper dependency identification
- Regular status updates

### Todo File Quality
- Current and accurate status information
- Clear priority assignments
- Proper cross-referencing
- Regular cleanup of completed tasks

## Template Files

### Task Template
Located at: `ai-agent-workflow/templates/task-template.md`

### Context Template
Located at: `ai-agent-workflow/templates/context-template.md`

## Troubleshooting

### Missing Context
If resuming work without context:
1. Review git log for recent changes
2. Check existing documentation
3. Run diagnostic tests
4. Create new context file with current understanding

### Conflicting Information
When context files contradict:
1. Check timestamps for most recent
2. Validate against actual codebase state
3. Update with current accurate information
4. Document resolution in context file

### System State Issues
For environment or dependency problems:
1. Document issue in context file
2. Include reproduction steps
3. Record solution for future reference
4. Update workflow documentation if needed

This workflow ensures consistent, comprehensive context management across all AI agent sessions while maintaining the ability to seamlessly resume complex development work.