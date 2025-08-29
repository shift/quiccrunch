#!/usr/bin/env python3
"""
Example: QuicCrunch Client Usage
This shows how the Python bindings will work once built with maturin.
"""

import asyncio
import sys
import os

# Add the python package to the path  
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'python'))

import quiccrunch

async def async_client_example():
    """Example using async/await API."""
    print("Async Client Example")
    print("=" * 30)
    
    client = quiccrunch.QuicClient()
    
    try:
        # Connect to server
        print("Connecting to server...")
        connection_id = await client.connect_async("127.0.0.1:50051")
        print(f"✓ Connected with ID: {connection_id}")
        
        # Send a message
        print("Sending message...")
        client.send_message(connection_id, "Hello, QuicCrunch!")
        print("✓ Message sent")
        
        # Check connections
        count = client.connection_count()
        print(f"✓ Active connections: {count}")
        
        # Disconnect
        print("Disconnecting...")
        client.disconnect(connection_id)
        print("✓ Disconnected")
        
    except quiccrunch.QuicCrunchError as e:
        print(f"✗ QuicCrunch error: {e.message} (code: {e.error_code})")
    except Exception as e:
        print(f"✗ Unexpected error: {e}")

def sync_client_example():
    """Example using synchronous API."""
    print("Sync Client Example")
    print("=" * 30)
    
    client = quiccrunch.QuicClient()
    
    try:
        # Connect to server (blocking)
        print("Connecting to server...")
        connection_id = client.connect("127.0.0.1:50051")
        print(f"✓ Connected with ID: {connection_id}")
        
        # Send a message
        print("Sending message...")
        client.send_message(connection_id, b"Hello from sync client!")
        print("✓ Message sent")
        
        # Disconnect
        print("Disconnecting...")
        client.disconnect(connection_id)
        print("✓ Disconnected")
        
    except quiccrunch.QuicCrunchError as e:
        print(f"✗ QuicCrunch error: {e.message} (code: {e.error_code})")
    except Exception as e:
        print(f"✗ Unexpected error: {e}")

async def async_server_example():
    """Example server using async/await API."""
    print("Async Server Example")
    print("=" * 30)
    
    server = quiccrunch.QuicServer()
    
    try:
        # Start listening
        print("Starting server on 127.0.0.1:50051...")
        server.listen("127.0.0.1:50051")
        print("✓ Server listening")
        
        # Run server (this would run indefinitely)
        print("Server would run here (serve_async)...")
        # await server.serve_async()
        
    except quiccrunch.QuicCrunchError as e:
        print(f"✗ QuicCrunch error: {e.message} (code: {e.error_code})")
    except Exception as e:
        print(f"✗ Unexpected error: {e}")

def main():
    print(f"QuicCrunch Python Bindings v{quiccrunch.get_version()}")
    print(f"Library initialized: {quiccrunch.initialize() or 'OK'}")
    print()
    
    # Run sync example
    sync_client_example()
    print()
    
    # Run async examples
    print("Running async examples...")
    asyncio.run(async_client_example())
    print()
    asyncio.run(async_server_example())
    print()
    
    print("Note: These examples show the expected API behavior.")
    print("To enable real QUIC functionality, build with: maturin develop")

if __name__ == "__main__":
    main()