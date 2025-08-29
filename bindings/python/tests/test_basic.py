import pytest
from quiccrunch import QuicClient, QuicServer, QuicCrunchError, get_version


def test_version():
    """Test that we can get the library version."""
    version = get_version()
    assert isinstance(version, str)
    assert len(version) > 0


def test_client_creation():
    """Test that we can create a client."""
    client = QuicClient()
    assert client is not None


def test_server_creation():
    """Test that we can create a server."""
    server = QuicServer()
    assert server is not None


def test_error_creation():
    """Test QuicCrunchError creation."""
    error = QuicCrunchError("test message", 42)
    assert error.message == "test message"
    assert error.error_code == 42
    assert str(error) == "test message"


def test_client_operations_not_implemented():
    """Test that client operations raise not implemented errors."""
    client = QuicClient()
    
    with pytest.raises(QuicCrunchError):
        client.connect("127.0.0.1:50051")
    
    with pytest.raises(QuicCrunchError):
        client.send_message(1, b"test")
    
    with pytest.raises(QuicCrunchError):
        client.disconnect(1)
    
    with pytest.raises(QuicCrunchError):
        client.connection_count()


@pytest.mark.asyncio
async def test_client_async_operations_not_implemented():
    """Test that async client operations raise not implemented errors."""
    client = QuicClient()
    
    with pytest.raises(QuicCrunchError):
        await client.connect_async("127.0.0.1:50051")


def test_server_operations_not_implemented():
    """Test that server operations raise not implemented errors."""
    server = QuicServer()
    
    with pytest.raises(QuicCrunchError):
        server.listen("127.0.0.1:50051")
    
    with pytest.raises(QuicCrunchError):
        server.serve()
    
    with pytest.raises(QuicCrunchError):
        server.connection_count()


@pytest.mark.asyncio
async def test_server_async_operations_not_implemented():
    """Test that async server operations raise not implemented errors."""
    server = QuicServer()
    
    with pytest.raises(QuicCrunchError):
        await server.serve_async()