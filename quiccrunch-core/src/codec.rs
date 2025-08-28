use capnp::{message, serialize};
use bytes::{Bytes, BytesMut};
use crate::error::{Result, TransportError};

pub trait Codec {
    fn encode<T>(&self, msg: T) -> Result<Bytes> 
    where
        T: serialize::Owned;
        
    fn decode<'a, T>(&self, bytes: &'a [u8]) -> Result<T>
    where
        T: message::Owned<'a>;
}

pub struct CapnpCodec;

impl Codec for CapnpCodec {
    fn encode<T>(&self, msg: T) -> Result<Bytes>
    where
        T: serialize::Owned,
    {
        let mut buf = BytesMut::new();
        serialize::write_message(&mut buf, &msg)
            .map_err(|e| TransportError::SerializationError(e.to_string()))?;
        Ok(buf.freeze())
    }

    fn decode<'a, T>(&self, bytes: &'a [u8]) -> Result<T>
    where
        T: message::Owned<'a>,
    {
        message::read_message(bytes, message::ReaderOptions::new())
            .map_err(|e| TransportError::SerializationError(e.to_string()))
    }
}