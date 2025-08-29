#!/bin/bash
# Optimized build script with intelligent caching
# Usage: ./scripts/build_optimized.sh [feature-set] [profile]

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
CACHE_DIR="$PROJECT_ROOT/target/cache"
BUILD_LOG="$PROJECT_ROOT/target/build.log"

# Default values
FEATURE_SET="${1:-default}"
PROFILE="${2:-dev}"
VERBOSE="${VERBOSE:-0}"

# Feature sets with their dependencies
declare -A FEATURE_SETS=(
    ["minimal"]="backend-tquic"
    ["standard"]="backend-tquic,memory-pools"
    ["performance"]="backend-tquic,lock-free-channels,memory-pools,simd-optimizations,adaptive-optimizations"
    ["full"]="backend-tquic,lock-free-channels,memory-pools,simd-optimizations,adaptive-optimizations,io-uring-advanced,telemetry"
    ["grpc-compare"]="backend-tquic,grpc-comparison"
    ["default"]="backend-tquic,adaptive-optimizations"
)

# Build profiles
declare -A PROFILES=(
    ["dev"]="--profile dev"
    ["dev-fast"]="--profile dev-fast" 
    ["release"]="--release"
    ["bench"]="--profile bench"
)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() {
    echo -e "${BLUE}[$(date +'%H:%M:%S')]${NC} $*" | tee -a "$BUILD_LOG"
}

error() {
    echo -e "${RED}[ERROR]${NC} $*" | tee -a "$BUILD_LOG"
    exit 1
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $*" | tee -a "$BUILD_LOG"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $*" | tee -a "$BUILD_LOG"
}

# Initialize cache directory
init_cache() {
    mkdir -p "$CACHE_DIR"
    log "Initialized cache directory: $CACHE_DIR"
}

# Calculate feature hash for cache key
calc_feature_hash() {
    local features="$1"
    echo -n "$features" | sha256sum | cut -d' ' -f1 | head -c 16
}

# Check if cached build exists and is valid
check_cache() {
    local features="$1"
    local profile="$2"
    local hash=$(calc_feature_hash "$features")
    local cache_file="$CACHE_DIR/${hash}_${profile}.marker"
    
    if [[ -f "$cache_file" ]]; then
        local cache_time=$(stat -c %Y "$cache_file" 2>/dev/null || echo 0)
        local src_time=$(find "$PROJECT_ROOT/quiccrunch-core/src" -name "*.rs" -newer "$cache_file" | wc -l)
        
        if [[ $src_time -eq 0 ]]; then
            log "Cache hit for features: $features (profile: $profile)"
            return 0
        else
            log "Cache invalidated due to source changes"
            rm -f "$cache_file"
        fi
    fi
    return 1
}

# Mark successful build in cache
mark_cache() {
    local features="$1"
    local profile="$2"
    local hash=$(calc_feature_hash "$features")
    local cache_file="$CACHE_DIR/${hash}_${profile}.marker"
    
    touch "$cache_file"
    echo "features=$features" > "$cache_file"
    echo "profile=$profile" >> "$cache_file"
    echo "timestamp=$(date -Iseconds)" >> "$cache_file"
    echo "rustc_version=$(rustc --version)" >> "$cache_file"
    
    log "Cached build: $cache_file"
}

# Build with optimized settings
build_with_features() {
    local features="$1"
    local profile_args="$2"
    
    log "Building with features: $features"
    log "Profile: $profile_args"
    
    # Set environment for optimal caching
    export CARGO_INCREMENTAL=1
    export RUSTC_WRAPPER=""    # Disable sccache conflicts
    
    # Build command with caching optimizations
    local build_cmd="cargo build"
    
    if [[ "$features" != "default" ]]; then
        build_cmd="$build_cmd --no-default-features --features=$features"
    fi
    
    build_cmd="$build_cmd $profile_args"
    
    if [[ $VERBOSE -eq 1 ]]; then
        build_cmd="$build_cmd --verbose"
    fi
    
    log "Executing: $build_cmd"
    
    # Run build in NixOS environment
    if nix develop -c -- $build_cmd 2>&1 | tee -a "$BUILD_LOG"; then
        success "Build completed successfully"
        mark_cache "$features" "$profile"
        return 0
    else
        error "Build failed"
        return 1
    fi
}

# Clean specific feature build artifacts
clean_feature_build() {
    local features="$1"
    local hash=$(calc_feature_hash "$features")
    
    log "Cleaning feature build: $features"
    
    # Remove feature-specific cache markers
    rm -f "$CACHE_DIR/${hash}_"*.marker
    
    # Clean dependency artifacts that might conflict
    nix develop -c -- cargo clean --package quiccrunch-core
    
    log "Cleaned feature build artifacts"
}

# Show available feature sets
show_feature_sets() {
    echo "Available feature sets:"
    for set in "${!FEATURE_SETS[@]}"; do
        echo "  $set: ${FEATURE_SETS[$set]}"
    done
}

# Show cache status
show_cache_status() {
    log "Cache status:"
    if [[ -d "$CACHE_DIR" ]] && [[ -n "$(ls -A "$CACHE_DIR" 2>/dev/null)" ]]; then
        for cache_file in "$CACHE_DIR"/*.marker; do
            if [[ -f "$cache_file" ]]; then
                local basename=$(basename "$cache_file" .marker)
                local features=$(grep "^features=" "$cache_file" | cut -d'=' -f2- || echo "unknown")
                local timestamp=$(grep "^timestamp=" "$cache_file" | cut -d'=' -f2- || echo "unknown")
                echo "  $basename: $features (built: $timestamp)"
            fi
        done
    else
        echo "  No cached builds found"
    fi
}

# Main execution
main() {
    cd "$PROJECT_ROOT"
    
    # Initialize
    init_cache
    
    # Handle special commands
    case "${1:-}" in
        "help"|"-h"|"--help")
            echo "Usage: $0 [feature-set] [profile]"
            echo ""
            show_feature_sets
            echo ""
            echo "Available profiles: ${!PROFILES[*]}"
            echo ""
            echo "Special commands:"
            echo "  clean [feature-set]  - Clean specific feature build"
            echo "  status               - Show cache status"
            echo "  help                 - Show this help"
            exit 0
            ;;
        "clean")
            if [[ -n "${2:-}" ]]; then
                if [[ -v "FEATURE_SETS[$2]" ]]; then
                    clean_feature_build "${FEATURE_SETS[$2]}"
                else
                    error "Unknown feature set: $2"
                fi
            else
                log "Cleaning all build artifacts"
                nix develop -c -- cargo clean
                rm -rf "$CACHE_DIR"
                success "All build artifacts cleaned"
            fi
            exit 0
            ;;
        "status")
            show_cache_status
            exit 0
            ;;
    esac
    
    # Validate feature set
    if [[ ! -v "FEATURE_SETS[$FEATURE_SET]" ]]; then
        error "Unknown feature set: $FEATURE_SET"
        show_feature_sets
        exit 1
    fi
    
    # Validate profile
    if [[ ! -v "PROFILES[$PROFILE]" ]]; then
        error "Unknown profile: $PROFILE"
        echo "Available profiles: ${!PROFILES[*]}"
        exit 1
    fi
    
    local features="${FEATURE_SETS[$FEATURE_SET]}"
    local profile_args="${PROFILES[$PROFILE]}"
    
    # Check cache first
    if check_cache "$features" "$PROFILE"; then
        success "Using cached build for $FEATURE_SET ($PROFILE)"
        exit 0
    fi
    
    # Build
    log "Starting optimized build for feature set: $FEATURE_SET"
    build_with_features "$features" "$profile_args"
    
    success "Build completed for feature set: $FEATURE_SET"
}

# Execute main with all arguments
main "$@"