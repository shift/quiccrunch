#!/usr/bin/env bash
set -euo pipefail

# Check for required utilities
for cmd in cpupower sensors dmidecode nix; do
    if ! command -v "$cmd" &> /dev/null; then
        echo "Error: Required command '$cmd' not found"
        exit 1
    fi
done

# Save current CPU configuration
echo "Saving current CPU configuration..."
ORIGINAL_GOVERNOR=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor)
ORIGINAL_MIN_FREQ=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_min_freq)
ORIGINAL_MAX_FREQ=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq)

function cleanup {
    echo "Restoring original CPU configuration..."
    echo "$ORIGINAL_GOVERNOR" > /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor
    echo "$ORIGINAL_MIN_FREQ" > /sys/devices/system/cpu/cpu0/cpufreq/scaling_min_freq
    echo "$ORIGINAL_MAX_FREQ" > /sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq
    sleep 2
}
trap cleanup EXIT

# Create results directory
RESULTS_DIR="target/benchmark/scaling_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

# Log system information
{
    echo "System Information"
    echo "=================="
    echo
    echo "CPU Information:"
    echo "---------------"
    lscpu
    
    echo
    echo "Memory Information:"
    echo "-----------------"
    free -h
    
    echo
    echo "DMI Information:"
    echo "---------------"
    dmidecode -t processor -t memory -t system
    
    echo
    echo "Initial Temperatures:"
    echo "-------------------"
    sensors
    
} > "$RESULTS_DIR/system_info.txt"

# Get frequency range
MIN_FREQ=$(cat /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_min_freq)
MAX_FREQ=$(cat /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq)
FREQ_RANGE=$((MAX_FREQ - MIN_FREQ))

# Calculate test frequencies
TEST_FREQS=(
    $MIN_FREQ                                    # Minimum
    $((MIN_FREQ + FREQ_RANGE / 4))              # 25%
    $((MIN_FREQ + FREQ_RANGE / 2))              # 50%
    $((MIN_FREQ + FREQ_RANGE * 3 / 4))          # 75%
    $MAX_FREQ                                    # Maximum
)

echo "Test frequencies (kHz): ${TEST_FREQS[*]}" >> "$RESULTS_DIR/system_info.txt"

# Test configurations
GOVERNORS=("performance" "powersave")

# Function to run a test configuration
run_test() {
    local freq=$1
    local gov=$2
    local test_name="freq${freq}_${gov}"
    local results_file="$RESULTS_DIR/${test_name}.txt"
    
    echo "Running test: $test_name"
    echo "Setting frequency to $freq kHz and governor to $gov"
    
    # Set CPU configuration
    echo "$gov" > /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor
    echo "$freq" > /sys/devices/system/cpu/cpu0/cpufreq/scaling_min_freq
    echo "$freq" > /sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq
    
    # Let system settle
    sleep 2
    
    # Record initial state
    {
        echo "Test Configuration"
        echo "================="
        echo "Frequency: $((freq/1000)) MHz"
        echo "Governor: $gov"
        echo
        echo "CPU Frequency Info:"
        cpupower frequency-info
        echo
        echo "Initial Temperatures:"
        sensors
    } > "$results_file"
    
    # Run the actual benchmark using nix-provided cargo
    echo "Running benchmark..."
    {
        echo
        echo "Benchmark Results"
        echo "================"
        RUST_BACKTRACE=1 nix develop -c cargo bench --bench patterns -- --nocapture
    } >> "$results_file" 2>&1  # Capture both stdout and stderr
    
    # Record final state
    {
        echo
        echo "Final System State"
        echo "================="
        echo
        echo "CPU Frequencies:"
        cpupower frequency-info
        echo
        echo "Temperatures:"
        sensors
        echo
        echo "Power Consumption:"
        echo "-----------------"
        if [ -d "/sys/class/powercap/intel-rapl" ]; then
            for domain in /sys/class/powercap/intel-rapl/intel-rapl:*/name; do
                domain_name=$(cat "$domain")
                energy_file="${domain%name}energy_uj"
                if [ -f "$energy_file" ]; then
                    energy_uj=$(cat "$energy_file")
                    energy_j=$(echo "scale=6; $energy_uj / 1000000" | bc)
                    echo "$domain_name: $energy_j Joules"
                fi
            done
        fi
    } >> "$results_file"
    
    # Let system cool down
    sleep 5
}

# Run all test configurations
for freq in "${TEST_FREQS[@]}"; do
    for gov in "${GOVERNORS[@]}"; do
        run_test "$freq" "$gov"
    done
done

# Generate summary report
{
    echo "Performance Scaling Test Results"
    echo "==============================="
    echo
    echo "Test completed at: $(date)"
    echo
    echo "System Configuration:"
    echo "-------------------"
    echo "CPU: $(lscpu | grep 'Model name' | cut -d':' -f2 | xargs)"
    echo "Memory: $(free -h | grep 'Mem:' | awk '{print $2}')"
    echo
    echo "Test Configurations Run:"
    echo "----------------------"
    for freq in "${TEST_FREQS[@]}"; do
        for gov in "${GOVERNORS[@]}"; do
            echo "- $((freq/1000))MHz with $gov governor"
        done
    done
    
    echo
    echo "Temperature Range:"
    echo "----------------"
    echo "Initial package temperature: $(sensors | grep 'Package id 0:' | cut -d'+' -f2 | cut -d'°' -f1)°C"
    echo "Final package temperature: $(sensors | grep 'Package id 0:' | cut -d'+' -f2 | cut -d'°' -f1)°C"
    
    echo
    echo "Power Consumption Summary:"
    echo "------------------------"
    if [ -d "/sys/class/powercap/intel-rapl" ]; then
        for domain in /sys/class/powercap/intel-rapl/intel-rapl:*/name; do
            domain_name=$(cat "$domain")
            energy_file="${domain%name}energy_uj"
            if [ -f "$energy_file" ]; then
                energy_uj=$(cat "$energy_file")
                energy_j=$(echo "scale=6; $energy_uj / 1000000" | bc)
                echo "$domain_name: $energy_j Joules"
            fi
        done
    fi
    
} > "$RESULTS_DIR/summary.md"

echo "Testing complete! Results available in $RESULTS_DIR"
echo "See $RESULTS_DIR/summary.md for overview"