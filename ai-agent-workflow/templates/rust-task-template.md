# Task: [Task Name]

## Objective
[Clear, specific goal statement that defines what success looks like for this Rust implementation]

## Acceptance Criteria
- [ ] [Measurable requirement 1 - must be verifiable]
- [ ] [Measurable requirement 2 - must be verifiable]
- [ ] [Measurable requirement 3 - must be verifiable]
- [ ] All tests pass (`cargo test` including integration tests)
- [ ] Code follows Rust conventions (rustfmt, clippy -D warnings)
- [ ] Documentation updated (`cargo doc`) where necessary
- [ ] No breaking changes to existing public APIs
- [ ] Performance benchmarks show no regressions
- [ ] Memory safety verified (no unsafe code unless absolutely necessary)

## Implementation Plan
1. [High-level step 1 with estimated effort - include Cargo.toml updates if needed]
2. [High-level step 2 with estimated effort - include module/file structure]
3. [High-level step 3 with estimated effort - include trait implementations]
4. [Testing and validation steps - include `cargo test`, `cargo bench`, integration tests]
5. [Documentation updates - include API docs, examples, README updates]

## Resources
- [Relevant Rust documentation links]
- [Example implementations or references in Rust ecosystem]
- [Related issues, PRs, or external resources]
- [Dependencies or prerequisites - include crate dependencies]

## Technical Details
- **Languages/Frameworks**: Rust 2021 edition, [specific crates/libraries]
- **Files to modify**: [Expected Rust files that will be changed - .rs files]
- **Testing approach**: [Unit tests, integration tests, benchmarks, fuzzing if applicable]
- **Risks**: [Ownership/borrowing issues, performance concerns, unsafe code usage]
- **Dependencies**: [Other tasks, external crates, system dependencies]

## Rust-Specific Requirements
- **Memory Safety**: No unsafe code unless justified and documented
- **Performance**: Zero-cost abstractions, efficient data structures
- **Error Handling**: Use `thiserror` for public error types, `anyhow` for internal
- **Async**: Use Tokio for async runtime, proper `Send`/`Sync` bounds
- **Documentation**: Comprehensive API docs with examples
- **Testing**: Unit tests, integration tests, property-based testing with `proptest`
- **Linting**: `cargo clippy -- -D warnings` must pass
- **Formatting**: `cargo fmt --all` must pass

## Status
- **Created**: [YYYY-MM-DD HH:MM TIMEZONE]
- **Priority**: [P0/P1/P2/P3]
- **Current Phase**: [Planning/Implementation/Testing/Documentation/Complete]
- **Assigned Agent**: [Implementation Agent Type - e.g., "Rust Performance Specialist"]
- **Blockers**: [None/Description of any blockers]
- **Next Steps**: [Immediate actions to take]

## Context Reference
[context/[task-name].md](../context/[task-name].md)

## Multi-Agent Workflow Status
- **Implementation Agent**: [Not started/In progress/Complete]
- **Testing Agent**: [Not started/In progress/Complete]
- **Documentation Agent**: [Not started/In progress/Complete]

---
*Task Definition for: [Task Name]*
*Rust Implementation Task*
*Last updated: [Current Timestamp]*