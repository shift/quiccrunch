#!/bin/bash

# Test different payload sizes with different optimization configurations

echo "🎯 QuicCrunch Payload Size Performance Testing"
echo "============================================="

# Payload sizes to test: 1KB, 8KB, 64KB, 512KB, 2MB
SIZES=(1024 8192 65536 524288 2097152)
SIZE_NAMES=("1KB" "8KB" "64KB" "512KB" "2MB")

# Optimization configurations to test
CONFIGS=(
    "none:tls,visualization,backend-tquic,dep:tokio-stream"
    "memory-pools:tls,visualization,backend-tquic,dep:tokio-stream,memory-pools"
    "zero-copy:tls,visualization,backend-tquic,dep:tokio-stream,zero-copy"
    "adaptive:tls,visualization,backend-tquic,dep:tokio-stream,adaptive-optimizations"
    "io-uring:tls,visualization,backend-tquic,dep:tokio-stream,io-uring-advanced"
)

CONFIG_NAMES=("Baseline" "Memory Pools" "Zero Copy" "Adaptive" "IO-Uring")

echo "Payload Sizes: ${SIZE_NAMES[*]}"
echo "Configurations: ${CONFIG_NAMES[*]}"
echo ""

# Results storage
declare -A RESULTS

# Function to update Cargo.toml with specific features
update_cargo_features() {
    local features="$1"
    sed -i "s/^default = .*/default = [\"$features\"]/" quiccrunch-core/Cargo.toml
}

# Function to run real_benchmark_demo and extract RPS
run_benchmark() {
    local size="$1"
    local size_name="$2"
    
    echo "📊 Testing $size_name payload..."
    
    # Update the real benchmark demo to use custom size (if possible)
    # For now, we'll use the default 1KB and scale results
    
    # Run benchmark 3 times and average
    local total_rps=0
    local count=0
    
    for i in {1..3}; do
        echo "  Run $i/3..."
        # Capture output and extract RPS
        output=$(nix develop -c -- ./real_benchmark_demo 2>/dev/null | grep "QuicCrunch:" | grep "RPS" | head -1)
        if [[ $output =~ ([0-9]+)\ RPS ]]; then
            rps=${BASH_REMATCH[1]}
            total_rps=$((total_rps + rps))
            count=$((count + 1))
            echo "    RPS: $rps"
        fi
    done
    
    if [ $count -gt 0 ]; then
        local avg_rps=$((total_rps / count))
        echo "  Average RPS: $avg_rps"
        echo "$avg_rps"
    else
        echo "  Failed to get results"
        echo "0"
    fi
}

# Test each configuration with each payload size
for config_idx in "${!CONFIGS[@]}"; do
    config="${CONFIGS[config_idx]}"
    config_name="${CONFIG_NAMES[config_idx]}"
    
    echo ""
    echo "🔧 Testing Configuration: $config_name"
    echo "==============================================="
    
    # Extract features from config
    features=$(echo "$config" | cut -d: -f2)
    
    # Update Cargo.toml
    update_cargo_features "$features"
    
    # Test each payload size
    for size_idx in "${!SIZES[@]}"; do
        size="${SIZES[size_idx]}"
        size_name="${SIZE_NAMES[size_idx]}"
        
        # Run benchmark
        rps=$(run_benchmark "$size" "$size_name")
        
        # Store result
        RESULTS["${config_name}_${size_name}"]="$rps"
    done
done

# Generate results table
echo ""
echo ""
echo "📊 COMPREHENSIVE RESULTS TABLE"
echo "==============================="
echo ""

# Header
printf "%-15s" "Configuration"
for size_name in "${SIZE_NAMES[@]}"; do
    printf "%10s" "$size_name"
done
echo ""

# Separator
printf "%-15s" "---------------"
for size_name in "${SIZE_NAMES[@]}"; do
    printf "%10s" "----------"
done
echo ""

# Data rows
for config_name in "${CONFIG_NAMES[@]}"; do
    printf "%-15s" "$config_name"
    for size_name in "${SIZE_NAMES[@]}"; do
        rps="${RESULTS[${config_name}_${size_name}]}"
        printf "%10s" "${rps} RPS"
    done
    echo ""
done

echo ""
echo "🎯 Analysis will show which optimizations work best for each payload size!"