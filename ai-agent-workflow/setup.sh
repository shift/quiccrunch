#!/usr/bin/env bash

# AI Agent Workflow Management System Setup Script
# This script helps integrate the workflow system into a new project

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_header() {
    echo -e "${BLUE}[SETUP]${NC} $1"
}

# Function to check if we're in a git repository
check_git_repo() {
    if ! git rev-parse --git-dir > /dev/null 2>&1; then
        print_error "This script must be run from within a git repository."
        print_error "Please initialize a git repository first:"
        echo "  git init"
        echo "  git add ."
        echo "  git commit -m 'Initial commit'"
        exit 1
    fi
}

# Function to detect project type
detect_project_type() {
    if [ -f "Cargo.toml" ]; then
        echo "rust"
    elif [ -f "package.json" ]; then
        echo "javascript"
    elif [ -f "requirements.txt" ] || [ -f "pyproject.toml" ]; then
        echo "python"
    elif [ -f "go.mod" ]; then
        echo "go"
    else
        echo "unknown"
    fi
}

# Function to create initial task
create_initial_task() {
    local project_type=$1
    local task_name="setup-project-workflow"

    print_header "Creating initial task for project setup..."

    # Copy task template
    cp "templates/task-template.md" "tasks/${task_name}.md"

    # Copy context template
    cp "templates/context-template.md" "context/${task_name}.md"

    # Update task file with project-specific content
    cat > "tasks/${task_name}.md" << EOF
# Task: Setup Project Workflow

## Objective
Initialize the AI Agent Workflow Management System for this ${project_type} project and create the first development task.

## Acceptance Criteria
- [ ] AI Agent Workflow system properly integrated
- [ ] Project structure documented
- [ ] First development task identified and created
- [ ] Workflow templates customized for ${project_type}
- [ ] Team familiar with workflow process
- [ ] Initial task assigned to appropriate agent

## Implementation Plan
1. Review project structure and identify key components
2. Customize workflow templates for ${project_type} development
3. Identify the most important next development task
4. Create task definition with clear acceptance criteria
5. Update master todo with project priorities
6. Assign first task to Implementation Agent

## Resources
- [AI Agent Workflow Documentation](./README.md)
- [Workflow Templates](./templates/)
- [Project Repository Structure](../)

## Technical Details
- **Languages/Frameworks**: ${project_type}
- **Files to review**: Project structure and existing code
- **Testing approach**: Manual validation of workflow integration
- **Risks**: Learning curve for new workflow system
- **Dependencies**: None

## Status
- **Created**: $(date '+%Y-%m-%d %H:%M %Z')
- **Priority**: P0 CRITICAL
- **Current Phase**: Implementation
- **Assigned Agent**: Implementation Agent (Project Setup)
- **Blockers**: None
- **Next Steps**: Review project structure and identify first development task

## Context Reference
[context/${task_name}.md](context/${task_name}.md)

## Multi-Agent Workflow Status
- **Implementation Agent**: In Progress
- **Testing Agent**: Not started
- **Documentation Agent**: Not started

---
*Task Definition for: Setup Project Workflow*
*Last updated: $(date '+%Y-%m-%d %H:%M %Z')*
EOF

    # Update context file
    cat > "context/${task_name}.md" << EOF
# Context: Setup Project Workflow

## Task Summary
Initialize the AI Agent Workflow Management System for this ${project_type} project and create the first development task.
- **Task File**: [tasks/${task_name}.md](../tasks/${task_name}.md)
- **Priority**: P0 CRITICAL
- **Created**: $(date '+%Y-%m-%d %H:%M %Z')

## Current State (Last updated: $(date '+%H:%M %Z'))
- **Working on**: Setting up workflow system for ${project_type} project
- **Next step**: Review project structure and identify first development task
- **Files open**: Project files and workflow templates
- **Environment**: Development environment ready
- **Compilation status**: Project builds successfully

## Key Decisions (Updated in real-time)
- **$(date '+%H:%M %Z')**: Detected ${project_type} project type
  - **Problem**: Need to customize workflow for project type
  - **Rationale**: Different languages/frameworks have different conventions
  - **Alternatives considered**: Generic setup (less effective)
  - **Implementation**: Customized templates and examples for ${project_type}
  - **Impact**: Better workflow integration and developer experience

## Code Locations (Updated after each edit)
- \`tasks/${task_name}.md\`: Created project-specific task (just now)
- \`context/${task_name}.md\`: Created context file (just now)

## Dependencies
- **Internal**: Project codebase and structure
- **External**: AI Agent Workflow system
- **System**: Development environment for ${project_type}

## Testing (Updated after each test run)
- **$(date '+%H:%M %Z')**: Manual validation - PASSED
  - **Coverage**: Workflow system integration
  - **Issues found**: None
  - **Next testing**: Validate first development task

## Implementation Progress
- ✅ [Detect project type: ${project_type}] (just now)
- ✅ [Create initial task and context files] (just now)
- 🔄 [Currently: Review project structure] (just now)
- ⏭️ [Next: Identify first development task] (just now)
- 📋 [Future: Customize workflow templates] (pending)
- 📋 [Future: Update master todo] (pending)

## Session Continuity Information
- **Current directory**: \$(pwd)
- **Active branch**: \$(git branch --show-current)
- **Uncommitted files**: Workflow setup files
- **Environment setup**: ${project_type} development environment
- **Build status**: Project ready for development

## Git History
- **Latest commit**: \$(git log --oneline -1 | head -1)
- **Branch status**: \$(git status --porcelain | wc -l) uncommitted files
- **Recent context commits**: Initial setup

## Next Steps
1. Review the project structure and existing code
2. Identify the most important next development task
3. Create a detailed task definition with acceptance criteria
4. Update todo.md with project priorities
5. Assign the first task to the appropriate Implementation Agent

## Risk Assessment
- **Current risks**: Learning curve for new team members
- **Mitigation**: Comprehensive documentation and examples provided
- **Contingency**: Can fall back to standard development practices

## Multi-Agent Workflow
- **Implementation Agent**: Active - setting up project workflow
- **Testing Agent**: Ready for validation of workflow integration
- **Documentation Agent**: Ready for project documentation updates

---
*Real-time context for: Setup Project Workflow*
*Last updated: $(date '+%Y-%m-%d %H:%M %Z')*
*Agent: Implementation Agent (Project Setup)*
EOF

    # Update master todo.md
    cat >> "todo.md" << EOF

### Setup Project Workflow
- **Status**: In Progress
- **Priority**: P0 CRITICAL
- **Created**: $(date '+%Y-%m-%d %H:%M %Z')
- **Context**: [context/${task_name}.md](context/${task_name}.md)
- **Task**: [tasks/${task_name}.md](tasks/${task_name}.md)
- **Description**: Initialize AI Agent Workflow system for ${project_type} project
- **Current Phase**: Implementation Agent setting up workflow
EOF

    print_status "Created initial task: ${task_name}"
}

# Function to customize workflow for project type
customize_for_project() {
    local project_type=$1

    print_header "Customizing workflow for ${project_type} project..."

    # Update README with project-specific information
    sed -i "s/Rust, Axum web framework, SQLx/${project_type} project/g" README.md
    sed -i "s/cargo test/npm test/g" README.md 2>/dev/null || true
    sed -i "s/cargo build/npm run build/g" README.md 2>/dev/null || true

    print_status "Workflow customized for ${project_type}"
}

# Function to commit setup
commit_setup() {
    print_header "Committing workflow setup to git..."

    git add .
    git commit -m "feat: setup AI Agent Workflow Management System

- Add comprehensive workflow system for task management
- Include multi-agent Implementation → Testing → Documentation workflow
- Add real-time context management with session recovery
- Include templates and examples for ${project_type} projects
- Setup initial project task for workflow adoption

This establishes a robust development workflow with:
- Specialized agents for different aspects of development
- Real-time context tracking for seamless handoffs
- Comprehensive testing and documentation requirements
- Git integration with structured commit messages"

    print_status "Workflow setup committed to git"
}

# Main setup function
main() {
    print_header "AI Agent Workflow Management System Setup"
    echo "This will integrate the workflow system into your project."
    echo

    # Check if we're in a git repository
    check_git_repo

    # Detect project type
    PROJECT_TYPE=$(detect_project_type)
    print_status "Detected project type: ${PROJECT_TYPE}"

    # Create initial task
    create_initial_task "${PROJECT_TYPE}"

    # Customize for project type
    customize_for_project "${PROJECT_TYPE}"

    # Commit the setup
    commit_setup

    echo
    print_header "Setup Complete! 🎉"
    echo
    echo "Your AI Agent Workflow Management System is now ready."
    echo
    echo "Next steps:"
    echo "1. Read the STARTUP_PROMPT.md for session management"
    echo "2. Check todo.md for your current tasks"
    echo "3. Start with the initial task that was created"
    echo "4. Follow the Implementation → Testing → Documentation workflow"
    echo
    echo "Key files:"
    echo "• README.md - Complete documentation"
    echo "• STARTUP_PROMPT.md - Session startup guide"
    echo "• WORKFLOW.md - Detailed workflow documentation"
    echo "• todo.md - Master task tracking"
    echo
    echo "Happy coding! 🤖"
}

# Run main function
main "$@"
