#!/bin/bash
# QuicCrunch Performance Crisis Investigation Script
# This script helps debug the 4x performance regression identified in gap analysis

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RESULTS_DIR="$PROJECT_ROOT/performance_investigation"

echo "🔍 QuicCrunch Performance Crisis Investigation"
echo "=============================================="
echo "Project root: $PROJECT_ROOT"
echo "Results dir: $RESULTS_DIR"
echo ""

# Create results directory
mkdir -p "$RESULTS_DIR"
cd "$PROJECT_ROOT"

# Function to run benchmark and capture results
run_benchmark() {
    local name="$1"
    local features="$2"
    local output_file="$RESULTS_DIR/${name}_$(date +%Y%m%d_%H%M%S).txt"
    
    echo "📊 Running $name benchmark..."
    echo "Features: $features"
    echo "Output: $output_file"
    
    {
        echo "=== $name Benchmark Results ==="
        echo "Date: $(date)"
        echo "Features: $features"
        echo "Git commit: $(git rev-parse HEAD)"
        echo ""
        
        if [[ "$features" == "none" ]]; then
            cargo run --example simple_benchmark 2>&1 || echo "Baseline benchmark failed"
        else
            cargo run --example simple_benchmark --features="$features" 2>&1 || echo "Optimized benchmark failed"
        fi
        
        echo ""
        echo "=== System Information ==="
        echo "CPU: $(lscpu | grep 'Model name' | cut -d':' -f2 | xargs)"
        echo "Memory: $(free -h | grep '^Mem:' | awk '{print $2}')"
        echo "Kernel: $(uname -r)"
        echo ""
    } | tee "$output_file"
    
    return $?
}

# Function to profile with perf if available
profile_benchmark() {
    local name="$1"
    local features="$2"
    
    if command -v perf >/dev/null 2>&1; then
        echo "🔬 Profiling $name with perf..."
        local perf_output="$RESULTS_DIR/${name}_profile_$(date +%Y%m%d_%H%M%S).perf"
        
        if [[ "$features" == "none" ]]; then
            sudo perf record -g -o "$perf_output" -- cargo run --example simple_benchmark 2>/dev/null || echo "Profiling failed"
        else
            sudo perf record -g -o "$perf_output" -- cargo run --example simple_benchmark --features="$features" 2>/dev/null || echo "Profiling failed"
        fi
        
        if [[ -f "$perf_output" ]]; then
            echo "Profile saved to: $perf_output"
            echo "Analyze with: perf report -i $perf_output"
        fi
    else
        echo "⚠️  perf not available, skipping profiling"
    fi
}

# Step 1: Establish baseline performance
echo "🎯 Step 1: Baseline Performance"
echo "==============================="
run_benchmark "baseline" "none"
echo ""

# Step 2: Test individual optimizations
echo "🧪 Step 2: Individual Optimization Testing"
echo "=========================================="

optimizations=(
    "lock-free-channels"
    "io-uring-advanced" 
    "zero-copy"
    "memory-pools"
    "simd-optimizations"
)

for opt in "${optimizations[@]}"; do
    echo "Testing optimization: $opt"
    run_benchmark "single_$opt" "$opt" || echo "Failed to test $opt"
    echo ""
done

# Step 3: Test combined optimizations (the problematic case)
echo "🔥 Step 3: Combined Optimizations (Problem Case)"
echo "==============================================="
run_benchmark "all_optimizations" "lock-free-channels,io-uring-advanced,zero-copy,memory-pools,simd-optimizations"
echo ""

# Step 4: Profile the problematic case
echo "🔬 Step 4: Profiling Analysis"
echo "============================"
profile_benchmark "baseline" "none"
profile_benchmark "optimized" "lock-free-channels,io-uring-advanced,zero-copy"
echo ""

# Step 5: Check threading and concurrency
echo "🧵 Step 5: Threading Analysis" 
echo "============================"
{
    echo "=== Threading Analysis ==="
    echo "Date: $(date)"
    echo ""
    
    echo "Checking for threading issues in optimizations..."
    
    # Look for threading-related code in optimizations
    echo "Lock-free channels threading:"
    grep -n "thread\|spawn\|async\|await" src/lockfree.rs 2>/dev/null | head -10 || echo "No threading patterns found"
    echo ""
    
    echo "io_uring advanced threading:"
    grep -n "thread\|spawn\|async\|await" src/io_uring_advanced.rs 2>/dev/null | head -10 || echo "No threading patterns found"
    echo ""
    
    echo "Main transport threading:"
    grep -n "thread\|spawn\|async\|await" src/transport.rs 2>/dev/null | head -10 || echo "No threading patterns found"
    echo ""
    
    echo "=== Current CPU count ==="
    nproc
    echo ""
    
    echo "=== Tokio runtime info ==="
    echo "Check tokio runtime configuration in benchmarks..."
    
} | tee "$RESULTS_DIR/threading_analysis_$(date +%Y%m%d_%H%M%S).txt"

# Step 6: Feature flag analysis
echo "🏁 Step 6: Feature Flag Analysis"
echo "==============================="
{
    echo "=== Feature Flag Analysis ==="
    echo "Date: $(date)"
    echo ""
    
    echo "Current Cargo.toml features:"
    grep -A 20 '\[features\]' Cargo.toml 2>/dev/null || echo "No features section found"
    echo ""
    
    echo "Checking for feature-gated code issues..."
    echo "Files with #[cfg(feature = ...)]:"
    find src/ -name "*.rs" -exec grep -l "cfg.*feature" {} \; 2>/dev/null || echo "No feature-gated files found"
    echo ""
    
} | tee "$RESULTS_DIR/feature_analysis_$(date +%Y%m%d_%H%M%S).txt"

# Step 7: Generate investigation summary
echo "📋 Step 7: Investigation Summary"
echo "==============================="
{
    echo "=== QuicCrunch Performance Crisis Investigation Summary ==="
    echo "Date: $(date)"
    echo "Investigator: $(whoami)"
    echo "Project: QuicCrunch high-performance gRPC alternative"
    echo ""
    
    echo "ISSUE: 4x performance degradation when optimizations enabled"
    echo "ROOT CAUSE HYPOTHESIS: Multi-threading → single-threading constraint"
    echo ""
    
    echo "INVESTIGATION RESULTS:"
    echo "1. Baseline performance: See baseline_*.txt"
    echo "2. Individual optimizations: See single_*_*.txt" 
    echo "3. Combined optimizations: See all_optimizations_*.txt"
    echo "4. Profiling data: See *_profile_*.perf files"
    echo "5. Threading analysis: See threading_analysis_*.txt"
    echo "6. Feature analysis: See feature_analysis_*.txt"
    echo ""
    
    echo "NEXT STEPS:"
    echo "1. Compare baseline vs optimized performance numbers"
    echo "2. Analyze perf profiles for threading bottlenecks"
    echo "3. Check if optimizations serialize parallel operations"
    echo "4. Fix threading model in optimization modules"
    echo "5. Re-test performance after fixes"
    echo ""
    
    echo "FILES GENERATED:"
    ls -la "$RESULTS_DIR"
    echo ""
    
} | tee "$RESULTS_DIR/investigation_summary_$(date +%Y%m%d_%H%M%S).txt"

echo "✅ Performance crisis investigation complete!"
echo ""
echo "📁 All results saved to: $RESULTS_DIR"
echo ""
echo "🔍 To analyze results:"
echo "  1. Compare performance numbers in baseline vs optimized files"
echo "  2. Look for threading issues in threading_analysis file"
echo "  3. Review feature flag conflicts in feature_analysis file"
echo "  4. If perf profiles were generated, analyze with:"
echo "     perf report -i $RESULTS_DIR/*_profile_*.perf"
echo ""
echo "🚨 Priority: Fix the 4x performance regression before implementing new features!"
echo "💡 Expected outcome: 5-15x performance improvement once optimizations work correctly"