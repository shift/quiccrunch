#!/usr/bin/env bash

echo "🔋 Testing Fixed Power Monitoring"
echo "================================="
echo "🚀 Default Backend: tQUIC (High-Performance)"

# Simple test - just read sensors to verify they work
echo "📊 Current system readings:"

echo -n "ThinkPad CPU Temperature: "
if [ -f "/sys/class/hwmon/hwmon6/temp1_input" ]; then
    temp=$(cat /sys/class/hwmon/hwmon6/temp1_input 2>/dev/null || echo "N/A")
    if [ "$temp" != "N/A" ]; then
        echo "$((temp / 1000))°C"
    else
        echo "N/A"
    fi
else
    echo "Sensor not found"
fi

echo -n "CPU Frequency: "
if [ -f "/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq" ]; then
    freq=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq 2>/dev/null || echo "N/A")
    if [ "$freq" != "N/A" ]; then
        echo "$((freq / 1000))MHz"
    else
        echo "N/A"
    fi
else
    echo "Sensor not found"
fi

echo -n "Power monitoring available: "
if [ -f "/sys/class/powercap/intel-rapl/intel-rapl:0/energy_uj" ]; then
    if [ -r "/sys/class/powercap/intel-rapl/intel-rapl:0/energy_uj" ]; then
        echo "✅ Yes (with root permissions)"
    else
        echo "⚠️  Yes (requires sudo)"
    fi
else
    echo "❌ No"
fi

echo ""
echo "🧪 To run the comprehensive power benchmark:"
echo "   Default (tQUIC): nix develop -c cargo run --example power_benchmark_comparison"  
echo "   Full analysis:   sudo nix develop -c cargo run --example power_benchmark_comparison"
echo ""
echo "📊 To compare with Quinn backend:"
echo "   Quinn test:      nix develop -c cargo run --example power_benchmark_comparison --no-default-features --features backend-quinn"