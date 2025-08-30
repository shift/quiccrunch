# AI Task Management Directory

This directory contains comprehensive task planning and context files for the gRPC vs QuicCrunch performance comparison project.

## Important AI Guidelines

### Code Preservation Rules
1. **NEVER remove existing code** - even if you don't understand its purpose
2. **Ask the human operator** when functionality is unclear instead of making assumptions
3. **Mocks in production code are FORBIDDEN** - only use mocks in dedicated test files
4. **Preserve all working functionality** - regression is unacceptable

### When in Doubt
- Stop and ask the human operator for clarification
- Don't assume code should be removed or simplified
- Respect existing architectural decisions

## Files

### `tasks.md`
Complete task breakdown including:
- Priority tasks with critical tQUIC backend fix
- Comprehensive test matrix (340 test scenarios)
- Implementation phases and timeline
- Success criteria and risk mitigation

### `context_files.md`
Quick reference guide containing:
- File locations and purposes
- Code structure and dependencies
- Current issues and implementation gaps
- Build commands and debugging tips

## Current Status

The project needs completion of:
1. **tQUIC backend integration** (critical blocker)
2. **Message size expansion** (16B to 2MB coverage)
3. **Streaming pattern implementation** (4 patterns)
4. **Comprehensive benchmark execution** (340 scenarios)

## Quick Start

1. Review `tasks.md` for complete work breakdown
2. Use `context_files.md` for file references
3. Start with Priority 1 (tQUIC backend fix)
4. Follow implementation phases

See individual files for detailed information.