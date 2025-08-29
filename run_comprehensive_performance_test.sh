#!/bin/bash

echo "🚀 QuicCrunch Comprehensive Performance Testing"
echo "================================================"
echo

# Get current date for file naming
DATE=$(date +%Y-%m-%d)

# Determine enabled features
FEATURES="backend-tquic"

echo "📅 Test Date: $DATE"
echo "🔧 Features: $FEATURES"
echo

# Create performance directory
mkdir -p performance

echo "🏗️  Building with optimizations..."
nix develop -c -- cargo build --release --features="$FEATURES" --no-default-features

if [ $? -ne 0 ]; then
    echo "❌ Build failed"
    exit 1
fi

echo "✅ Build successful"
echo

echo "🧪 Running comprehensive performance test..."
nix develop -c -- cargo run --release --features="$FEATURES" --no-default-features --bin comprehensive_performance_runner

if [ $? -ne 0 ]; then
    echo "❌ Performance test failed"
    exit 1
fi

echo "✅ Performance test completed successfully"
echo

echo "📁 Results saved in:"
ls -la performance/${DATE}-*.md performance/${DATE}-*.json 2>/dev/null | head -5

echo
echo "🎉 Performance testing complete!"
echo "Check the performance/ directory for detailed results and reports."