#!/usr/bin/env python3
"""
Simple test script for QuicCrunch Python bindings.
This demonstrates the basic API structure before the Rust module is built.
"""

import sys
import os

# Add the python package to the path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'python'))

try:
    import quiccrunch
    
    print("QuicCrunch Python Bindings Test")
    print("=" * 40)
    print(f"Version: {quiccrunch.get_version()}")
    print()
    
    # Test client creation
    print("Creating QuicClient...")
    client = quiccrunch.QuicClient()
    print("✓ Client created successfully")
    print()
    
    # Test server creation  
    print("Creating QuicServer...")
    server = quiccrunch.QuicServer()
    print("✓ Server created successfully")
    print()
    
    # Test error handling
    print("Testing error handling...")
    try:
        client.connect("127.0.0.1:50051")
    except quiccrunch.QuicCrunchError as e:
        print(f"✓ Expected error caught: {e.message}")
    print()
    
    print("Basic API structure is working!")
    print("Next step: Build the Rust bindings with maturin")
    print()
    print("To build:")
    print("1. Install maturin: pip install maturin")
    print("2. Build bindings: maturin develop")
    print("3. Test with real QUIC functionality")
    
except ImportError as e:
    print(f"Import error: {e}")
    sys.exit(1)
except Exception as e:
    print(f"Unexpected error: {e}")
    sys.exit(1)