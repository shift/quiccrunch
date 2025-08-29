"""
QuicCrunch Python Bindings

High-performance QUIC transport library with Cap'n Proto serialization.
"""

from typing import Optional, Union, List, Dict, Any
import asyncio

__version__ = "0.1.0"

class QuicCrunchError(Exception):
    """Base exception for QuicCrunch errors."""
    def __init__(self, message: str, error_code: int = 0):
        super().__init__(message)
        self.message = message
        self.error_code = error_code


class QuicClient:
    """High-performance QUIC client with async/await support."""
    
    def __init__(self):
        """Create a new QUIC client."""
        # This will be replaced when the Rust module is built
        self._client = None
    
    def connect(self, address: str) -> int:
        """
        Connect to a QUIC server (blocking).
        
        Args:
            address: Server address in format "host:port"
            
        Returns:
            Connection ID for the established connection
            
        Raises:
            QuicCrunchError: If connection fails
        """
        raise QuicCrunchError("Python bindings not yet built")
    
    async def connect_async(self, address: str) -> int:
        """
        Connect to a QUIC server (async).
        
        Args:
            address: Server address in format "host:port"
            
        Returns:
            Connection ID for the established connection
            
        Raises:
            QuicCrunchError: If connection fails
        """
        raise QuicCrunchError("Python bindings not yet built")
    
    def send_message(self, connection_id: int, data: Union[str, bytes]) -> None:
        """
        Send a message over the connection.
        
        Args:
            connection_id: Connection ID from connect()
            data: Message data to send
            
        Raises:
            QuicCrunchError: If sending fails
        """
        if isinstance(data, str):
            data = data.encode('utf-8')
        raise QuicCrunchError("Python bindings not yet built")
    
    def disconnect(self, connection_id: int) -> None:
        """
        Disconnect from the server.
        
        Args:
            connection_id: Connection ID to disconnect
            
        Raises:
            QuicCrunchError: If disconnect fails
        """
        raise QuicCrunchError("Python bindings not yet built")
    
    def connection_count(self) -> int:
        """
        Get the number of active connections.
        
        Returns:
            Number of active connections
        """
        raise QuicCrunchError("Python bindings not yet built")


class QuicServer:
    """High-performance QUIC server with async/await support."""
    
    def __init__(self):
        """Create a new QUIC server."""
        # This will be replaced when the Rust module is built
        self._server = None
    
    def listen(self, address: str) -> None:
        """
        Start listening on the specified address.
        
        Args:
            address: Address to listen on in format "host:port"
            
        Raises:
            QuicCrunchError: If listening fails
        """
        raise QuicCrunchError("Python bindings not yet built")
    
    def serve(self) -> None:
        """
        Run the server (blocking).
        
        This will block until the server is stopped.
        
        Raises:
            QuicCrunchError: If server fails
        """
        raise QuicCrunchError("Python bindings not yet built")
    
    async def serve_async(self) -> None:
        """
        Run the server (async).
        
        This will run until cancelled.
        
        Raises:
            QuicCrunchError: If server fails
        """
        raise QuicCrunchError("Python bindings not yet built")
    
    def connection_count(self) -> int:
        """
        Get the number of active connections.
        
        Returns:
            Number of active connections
        """
        raise QuicCrunchError("Python bindings not yet built")


# Convenience functions
def get_version() -> str:
    """Get the QuicCrunch library version."""
    return __version__


def initialize() -> None:
    """Initialize the QuicCrunch library (called automatically on import)."""
    pass  # Will be implemented when Rust module is built


# Export public API
__all__ = [
    "QuicClient",
    "QuicServer", 
    "QuicCrunchError",
    "get_version",
    "initialize",
    "__version__",
]