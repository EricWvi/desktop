use crate::message::ValidateMessage;
use crate::{ControllerToNodeMessage, MessageValidationError, NodeToControllerMessage};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::io;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Frame tag identifying a Controller–Node JSON envelope.
pub const NODE_MESSAGE_FRAME_TYPE: u8 = 0x01;
/// Largest complete Controller–Node frame, including its one-byte type tag.
pub const MAX_FRAME_LENGTH: usize = 16 * 1024 * 1024;

/// Distinguishes transport truncation, framing failures, JSON failures, and protocol validation.
#[derive(Debug, Error)]
pub enum FrameError {
    #[error("Controller–Node frame I/O failed")]
    Io(#[from] io::Error),
    #[error("Controller–Node frame length {length} is outside the supported range")]
    InvalidLength { length: usize },
    #[error("unsupported Controller–Node frame type {frame_type}")]
    UnsupportedFrameType { frame_type: u8 },
    #[error("failed to encode Controller–Node message as JSON")]
    EncodeJson(#[source] serde_json::Error),
    #[error("failed to decode Controller–Node message JSON")]
    DecodeJson(#[source] serde_json::Error),
    #[error(transparent)]
    InvalidMessage(#[from] MessageValidationError),
}

/// Reads and validates one message sent from a Controller, returning `None` only at clean EOF.
pub async fn read_controller_message<R>(
    reader: &mut R,
) -> Result<Option<ControllerToNodeMessage>, FrameError>
where
    R: AsyncRead + Unpin,
{
    read_message(reader).await
}

/// Encodes and writes one validated message sent from a Controller.
pub async fn write_controller_message<W>(
    writer: &mut W,
    message: &ControllerToNodeMessage,
) -> Result<(), FrameError>
where
    W: AsyncWrite + Unpin,
{
    write_message(writer, message).await
}

/// Reads and validates one message sent from a Node, returning `None` only at clean EOF.
pub async fn read_node_message<R>(
    reader: &mut R,
) -> Result<Option<NodeToControllerMessage>, FrameError>
where
    R: AsyncRead + Unpin,
{
    read_message(reader).await
}

/// Encodes and writes one validated message sent from a Node.
pub async fn write_node_message<W>(
    writer: &mut W,
    message: &NodeToControllerMessage,
) -> Result<(), FrameError>
where
    W: AsyncWrite + Unpin,
{
    write_message(writer, message).await
}

/// Shares decoding and invariant checks while public functions retain peer direction in their type.
async fn read_message<R, Message>(reader: &mut R) -> Result<Option<Message>, FrameError>
where
    R: AsyncRead + Unpin,
    Message: DeserializeOwned + ValidateMessage,
{
    let Some(payload) = read_frame(reader).await? else {
        return Ok(None);
    };
    let message = serde_json::from_slice::<Message>(&payload).map_err(FrameError::DecodeJson)?;
    message.validate()?;
    Ok(Some(message))
}

/// Shares encoding and invariant checks while public functions retain peer direction in their type.
async fn write_message<W, Message>(writer: &mut W, message: &Message) -> Result<(), FrameError>
where
    W: AsyncWrite + Unpin,
    Message: Serialize + ValidateMessage,
{
    message.validate()?;
    let payload = serde_json::to_vec(message).map_err(FrameError::EncodeJson)?;
    write_frame(writer, &payload).await
}

/// Reads framing metadata before allocating the bounded JSON payload.
async fn read_frame<R>(reader: &mut R) -> Result<Option<Vec<u8>>, FrameError>
where
    R: AsyncRead + Unpin,
{
    let mut length_bytes = [0_u8; 4];
    match reader.read_u8().await {
        Ok(first_byte) => length_bytes[0] = first_byte,
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(FrameError::Io(error)),
    }
    reader.read_exact(&mut length_bytes[1..]).await?;

    let length = u32::from_be_bytes(length_bytes) as usize;
    if !(1..=MAX_FRAME_LENGTH).contains(&length) {
        return Err(FrameError::InvalidLength { length });
    }

    let frame_type = reader.read_u8().await?;
    if frame_type != NODE_MESSAGE_FRAME_TYPE {
        return Err(FrameError::UnsupportedFrameType { frame_type });
    }

    let payload_length = length - 1;
    let mut payload = vec![0_u8; payload_length];
    reader.read_exact(&mut payload).await?;
    Ok(Some(payload))
}

/// Writes one validated JSON payload using the protocol's binary frame envelope.
async fn write_frame<W>(writer: &mut W, payload: &[u8]) -> Result<(), FrameError>
where
    W: AsyncWrite + Unpin,
{
    let length = payload
        .len()
        .checked_add(/*rhs*/ 1)
        .filter(|length| *length <= MAX_FRAME_LENGTH)
        .ok_or(FrameError::InvalidLength {
            length: payload.len().saturating_add(/*rhs*/ 1),
        })?;
    let length = u32::try_from(length).map_err(|_| FrameError::InvalidLength { length })?;

    writer.write_all(&length.to_be_bytes()).await?;
    writer.write_u8(NODE_MESSAGE_FRAME_TYPE).await?;
    writer.write_all(payload).await?;
    writer.flush().await?;
    Ok(())
}
