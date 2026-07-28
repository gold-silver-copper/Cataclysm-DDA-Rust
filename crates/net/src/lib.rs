//! Shared iroh identity storage and bounded reliable framing. This crate has no
//! gameplay authority and no Bevy dependency.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use cdda_protocol::{
    ActorId, ControlMessage, MAX_BULK_DECODED, MAX_BULK_ENCODED, MAX_CONTROL_ENCODED,
    ReplicationSnapshotV1, decode_control, decode_replication_snapshot, encode_control,
    encode_replication_snapshot,
};
use iroh::{
    SecretKey,
    endpoint::{RecvStream, SendStream},
};

pub fn load_or_create_secret_key(path: impl AsRef<Path>) -> Result<SecretKey, IdentityError> {
    let path = path.as_ref();
    match OpenOptions::new().read(true).open(path) {
        Ok(mut file) => read_secret_key(&mut file),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let key = SecretKey::generate();
            let mut options = OpenOptions::new();
            options.create_new(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(path) {
                Ok(mut file) => {
                    file.write_all(&key.to_bytes()).map_err(IdentityError::Io)?;
                    file.sync_all().map_err(IdentityError::Io)?;
                    if let Some(parent) = path.parent()
                        && !parent.as_os_str().is_empty()
                    {
                        sync_directory(parent)?;
                    }
                    Ok(key)
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let mut file = OpenOptions::new()
                        .read(true)
                        .open(path)
                        .map_err(IdentityError::Io)?;
                    read_secret_key(&mut file)
                }
                Err(error) => Err(IdentityError::Io(error)),
            }
        }
        Err(error) => Err(IdentityError::Io(error)),
    }
}

fn read_secret_key(file: &mut fs::File) -> Result<SecretKey, IdentityError> {
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(IdentityError::Io)?;
    let exact: [u8; 32] = bytes.try_into().map_err(|_| IdentityError::InvalidLength)?;
    Ok(SecretKey::from_bytes(&exact))
}

fn sync_directory(path: &Path) -> Result<(), IdentityError> {
    #[cfg(unix)]
    {
        fs::File::open(path)
            .and_then(|directory| directory.sync_all())
            .map_err(IdentityError::Io)?;
    }
    #[cfg(not(unix))]
    let _unused = path;
    Ok(())
}

#[derive(Debug)]
pub enum IdentityError {
    InvalidLength,
    Io(std::io::Error),
}

impl fmt::Display for IdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength => formatter.write_str("iroh secret key must be exactly 32 bytes"),
            Self::Io(error) => write!(formatter, "iroh identity I/O error: {error}"),
        }
    }
}

impl std::error::Error for IdentityError {}

pub async fn write_control_frame(
    send: &mut SendStream,
    message: &ControlMessage,
) -> Result<(), FrameIoError> {
    let payload =
        encode_control(message).map_err(|error| FrameIoError::Codec(error.to_string()))?;
    let length = u32::try_from(payload.len()).map_err(|_| FrameIoError::TooLarge)?;
    send.write_all(&length.to_be_bytes())
        .await
        .map_err(|error| FrameIoError::Transport(error.to_string()))?;
    send.write_all(&payload)
        .await
        .map_err(|error| FrameIoError::Transport(error.to_string()))
}

pub async fn read_control_frame(receive: &mut RecvStream) -> Result<ControlMessage, FrameIoError> {
    let mut length = [0_u8; 4];
    receive
        .read_exact(&mut length)
        .await
        .map_err(|error| FrameIoError::Transport(error.to_string()))?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_CONTROL_ENCODED {
        return Err(FrameIoError::TooLarge);
    }
    let mut payload = vec![0; length];
    receive
        .read_exact(&mut payload)
        .await
        .map_err(|error| FrameIoError::Transport(error.to_string()))?;
    decode_control(&payload).map_err(|error| FrameIoError::Codec(error.to_string()))
}

pub async fn write_snapshot_stream(
    send: &mut SendStream,
    actor_id: ActorId,
    sequence: u64,
    snapshot: &ReplicationSnapshotV1,
) -> Result<(), FrameIoError> {
    let tick = snapshot.tick;
    let snapshot = snapshot.clone();
    let (encoded, encoded_length, decoded_length) = tokio::task::spawn_blocking(move || {
        let decoded = encode_replication_snapshot(&snapshot)
            .map_err(|error| FrameIoError::Codec(error.to_string()))?;
        let encoded = zstd::stream::encode_all(decoded.as_slice(), 3)
            .map_err(|error| FrameIoError::Codec(error.to_string()))?;
        if encoded.len() > MAX_BULK_ENCODED {
            return Err(FrameIoError::TooLarge);
        }
        let encoded_length = u32::try_from(encoded.len()).map_err(|_| FrameIoError::TooLarge)?;
        let decoded_length = u32::try_from(decoded.len()).map_err(|_| FrameIoError::TooLarge)?;
        Ok((encoded, encoded_length, decoded_length))
    })
    .await
    .map_err(|error| FrameIoError::Worker(error.to_string()))??;
    send.set_priority(-10)
        .map_err(|error| FrameIoError::Transport(error.to_string()))?;
    write_control_frame(
        send,
        &ControlMessage::SnapshotStreamReady {
            actor_id,
            sequence,
            tick,
            encoded_length,
            decoded_length,
        },
    )
    .await?;
    send.write_all(&encoded)
        .await
        .map_err(|error| FrameIoError::Transport(error.to_string()))?;
    send.finish()
        .map_err(|error| FrameIoError::Transport(error.to_string()))
}

pub async fn read_snapshot_stream(
    receive: &mut RecvStream,
) -> Result<(ActorId, u64, ReplicationSnapshotV1), FrameIoError> {
    let ControlMessage::SnapshotStreamReady {
        actor_id,
        sequence,
        tick,
        encoded_length,
        decoded_length,
    } = read_control_frame(receive).await?
    else {
        return Err(FrameIoError::Codec(String::from(
            "snapshot stream has an invalid header",
        )));
    };
    let encoded_length = encoded_length as usize;
    let decoded_length = decoded_length as usize;
    if encoded_length > MAX_BULK_ENCODED || decoded_length > MAX_BULK_DECODED {
        return Err(FrameIoError::TooLarge);
    }
    let mut encoded = vec![0; encoded_length];
    receive
        .read_exact(&mut encoded)
        .await
        .map_err(|error| FrameIoError::Transport(error.to_string()))?;
    let snapshot = tokio::task::spawn_blocking(move || {
        let decoder = zstd::stream::read::Decoder::new(encoded.as_slice())
            .map_err(|error| FrameIoError::Codec(error.to_string()))?;
        let mut decoded = Vec::with_capacity(decoded_length.min(MAX_BULK_DECODED));
        decoder
            .take((MAX_BULK_DECODED + 1) as u64)
            .read_to_end(&mut decoded)
            .map_err(|error| FrameIoError::Codec(error.to_string()))?;
        if decoded.len() != decoded_length || decoded.len() > MAX_BULK_DECODED {
            return Err(FrameIoError::TooLarge);
        }
        decode_replication_snapshot(&decoded)
            .map_err(|error| FrameIoError::Codec(error.to_string()))
    })
    .await
    .map_err(|error| FrameIoError::Worker(error.to_string()))??;
    if snapshot.tick != tick {
        return Err(FrameIoError::Codec(String::from(
            "snapshot stream tick does not match its header",
        )));
    }
    Ok((actor_id, sequence, snapshot))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FrameIoError {
    Codec(String),
    TooLarge,
    Transport(String),
    Worker(String),
}

impl fmt::Display for FrameIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Codec(error) => write!(formatter, "invalid control frame: {error}"),
            Self::TooLarge => formatter.write_str("control frame exceeds its size limit"),
            Self::Transport(error) => write!(formatter, "iroh stream error: {error}"),
            Self::Worker(error) => write!(formatter, "snapshot codec worker failed: {error}"),
        }
    }
}

impl std::error::Error for FrameIoError {}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_PATH: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn identity_is_stable_across_restarts() {
        let number = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "cdda-rust-net-identity-{}-{number}",
            std::process::id()
        ));
        let first = load_or_create_secret_key(&path).expect("identity should be created");
        let second = load_or_create_secret_key(&path).expect("identity should be reloaded");
        assert_eq!(first.public(), second.public());
        fs::remove_file(path).expect("temporary identity should be removable");
    }
}
