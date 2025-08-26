#!/usr/bin/env bash
set -euo pipefail

# Get original user for permission fixes
ORIGINAL_USER=${SUDO_USER:-$USER}

# Check for root privileges
if [ "$EUID" -ne 0 ]; then
    echo "Please run as root (sudo) to access CPU frequency controls"
    exit 1
fi

# Check for required tools
for cmd in dmidecode sensors cpupower; do
    if ! command -v $cmd &> /dev/null; then
        echo "Error: Required command '$cmd' not found"
        exit 1
    fi
done

# Check CPU frequency control access
if [ ! -w "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor" ]; then
    echo "Error: No write access to CPU frequency controls"
    exit 1
fi

# Save current CPU configuration
echo "Saving current CPU configuration..."
ORIGINAL_GOVERNOR=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor)
ORIGINAL_MIN_FREQ=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_min_freq)
ORIGINAL_MAX_FREQ=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq)

# Function to restore original settings
function cleanup {
    # Restore original CPU settings
    echo "Restoring original CPU configuration..."
    echo "$ORIGINAL_GOVERNOR" > /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor
    echo "$ORIGINAL_MIN_FREQ" > /sys/devices/system/cpu/cpu0/cpufreq/scaling_min_freq
    echo "$ORIGINAL_MAX_FREQ" > /sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq
    
    # Restore git repository permissions
    if [ -d "/home/shift/code/quiccrunch/.git" ]; then
        chown -R "${ORIGINAL_USER}" /home/shift/code/quiccrunch/.git
    fi
    
    # Give system time to settle
    sleep 2
    
    echo "Cleanup complete"
}

# Register cleanup function
trap cleanup EXIT

echo "Starting benchmark with system information:"
echo "----------------------------------------"
echo "CPU Info:"
lscpu | grep "Model name\|CPU(s)\|Thread(s) per core\|CPU MHz\|CPU max MHz\|CPU min MHz"
echo
echo "Memory Info:"
free -h
echo
echo "Current CPU Frequencies:"
cpupower frequency-info | grep "current CPU frequency"
echo
echo "Current Temperatures:"
sensors
echo
echo "----------------------------------------"

# Fix git repository permissions
echo "Fixing git repository permissions..."
if [ -d "/home/shift/code/quiccrunch/.git" ]; then
    chown -R "${ORIGINAL_USER}" /home/shift/code/quiccrunch/.git
fi

# Run the benchmark
echo "Running scaling benchmark..."
RUST_BACKTRACE=1 cargo test run_frequency_scaling -- --test-threads=1 --nocapture

echo "Benchmark complete!"