use std::io::{Read, Write};

use catcher_core::CatcherError;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

use crate::types::ws::{ApplicationCompressionAlgorithm, ApplicationCompressionConfig};

pub const APPLICATION_COMPRESSION_MAGIC: &[u8] = b"CATCHER-CMP-1";

const ALGORITHM_GZIP: u8 = 1;
const ALGORITHM_ZSTD: u8 = 2;
const KIND_TEXT: u8 = 1;
const KIND_BINARY: u8 = 2;
const LENGTH_BYTES: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationCompressionFrame {
    pub data: Vec<u8>,
    pub is_binary: bool,
}

pub fn encode_application_compression_frame(
    data: &[u8],
    is_binary: bool,
    config: &ApplicationCompressionConfig,
) -> Result<Option<Vec<u8>>, CatcherError> {
    if !config.enabled || data.len() < config.threshold_bytes as usize {
        return Ok(None);
    }
    if data.len() > u32::MAX as usize {
        return Err(CatcherError::EncodeError(
            "application compression payload exceeds u32 length".into(),
        ));
    }

    let compressed = match config.algorithm {
        ApplicationCompressionAlgorithm::Gzip => gzip_compress(data)?,
        ApplicationCompressionAlgorithm::Zstd => zstd_compress(data)?,
    };

    let mut frame = Vec::with_capacity(
        APPLICATION_COMPRESSION_MAGIC.len() + 2 + LENGTH_BYTES + compressed.len(),
    );
    frame.extend_from_slice(APPLICATION_COMPRESSION_MAGIC);
    frame.push(algorithm_to_wire(config.algorithm));
    frame.push(if is_binary { KIND_BINARY } else { KIND_TEXT });
    frame.extend_from_slice(&(data.len() as u32).to_be_bytes());
    frame.extend_from_slice(&compressed);
    Ok(Some(frame))
}

pub fn decode_application_compression_frame(
    data: &[u8],
    max_payload_bytes: u64,
) -> Result<Option<ApplicationCompressionFrame>, CatcherError> {
    if !data.starts_with(APPLICATION_COMPRESSION_MAGIC) {
        return Ok(None);
    }

    let header_len = APPLICATION_COMPRESSION_MAGIC.len() + 2 + LENGTH_BYTES;
    if data.len() < header_len {
        return Err(CatcherError::DecodeError(
            "application compression frame is truncated".into(),
        ));
    }

    let algorithm = wire_to_algorithm(data[APPLICATION_COMPRESSION_MAGIC.len()])?;
    let kind = data[APPLICATION_COMPRESSION_MAGIC.len() + 1];
    let len_start = APPLICATION_COMPRESSION_MAGIC.len() + 2;
    let expected_len = u32::from_be_bytes([
        data[len_start],
        data[len_start + 1],
        data[len_start + 2],
        data[len_start + 3],
    ]) as usize;

    if expected_len as u64 > max_payload_bytes {
        return Err(CatcherError::DecodeError(format!(
            "application compression frame expands beyond max payload: {expected_len} > {max_payload_bytes}",
        )));
    }

    let payload = &data[header_len..];
    let decoded = match algorithm {
        ApplicationCompressionAlgorithm::Gzip => {
            gzip_decompress(payload, max_payload_bytes as usize)?
        }
        ApplicationCompressionAlgorithm::Zstd => {
            zstd_decompress(payload, max_payload_bytes as usize)?
        }
    };

    if decoded.len() != expected_len {
        return Err(CatcherError::DecodeError(format!(
            "application compression length mismatch: expected {expected_len}, got {}",
            decoded.len(),
        )));
    }

    let is_binary = match kind {
        KIND_TEXT => false,
        KIND_BINARY => true,
        other => {
            return Err(CatcherError::DecodeError(format!(
                "unknown application compression message kind: {other}",
            )));
        }
    };

    Ok(Some(ApplicationCompressionFrame {
        data: decoded,
        is_binary,
    }))
}

fn algorithm_to_wire(algorithm: ApplicationCompressionAlgorithm) -> u8 {
    match algorithm {
        ApplicationCompressionAlgorithm::Gzip => ALGORITHM_GZIP,
        ApplicationCompressionAlgorithm::Zstd => ALGORITHM_ZSTD,
    }
}

fn wire_to_algorithm(value: u8) -> Result<ApplicationCompressionAlgorithm, CatcherError> {
    match value {
        ALGORITHM_GZIP => Ok(ApplicationCompressionAlgorithm::Gzip),
        ALGORITHM_ZSTD => Ok(ApplicationCompressionAlgorithm::Zstd),
        other => Err(CatcherError::DecodeError(format!(
            "unknown application compression algorithm: {other}",
        ))),
    }
}

fn gzip_compress(data: &[u8]) -> Result<Vec<u8>, CatcherError> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::new(6));
    encoder
        .write_all(data)
        .map_err(|e| CatcherError::EncodeError(format!("gzip write: {e}")))?;
    encoder
        .finish()
        .map_err(|e| CatcherError::EncodeError(format!("gzip finish: {e}")))
}

fn gzip_decompress(data: &[u8], max_payload_bytes: usize) -> Result<Vec<u8>, CatcherError> {
    let decoder = GzDecoder::new(data);
    read_limited(decoder, max_payload_bytes, "gzip")
}

fn zstd_compress(data: &[u8]) -> Result<Vec<u8>, CatcherError> {
    zstd::stream::encode_all(data, 3)
        .map_err(|e| CatcherError::EncodeError(format!("zstd encode: {e}")))
}

fn zstd_decompress(data: &[u8], max_payload_bytes: usize) -> Result<Vec<u8>, CatcherError> {
    let decoder = zstd::stream::Decoder::new(data)
        .map_err(|e| CatcherError::DecodeError(format!("zstd decoder: {e}")))?;
    read_limited(decoder, max_payload_bytes, "zstd")
}

fn read_limited<R: Read>(
    reader: R,
    max_payload_bytes: usize,
    label: &str,
) -> Result<Vec<u8>, CatcherError> {
    let mut limited = reader.take(max_payload_bytes as u64 + 1);
    let mut decoded = Vec::new();
    limited
        .read_to_end(&mut decoded)
        .map_err(|e| CatcherError::DecodeError(format!("{label} decode: {e}")))?;
    if decoded.len() > max_payload_bytes {
        return Err(CatcherError::DecodeError(format!(
            "{label} frame expands beyond max payload",
        )));
    }
    Ok(decoded)
}
