use catcher_ws::types::ws::{ApplicationCompressionAlgorithm, ApplicationCompressionConfig};
use catcher_ws::ws::{
    decode_application_compression_frame, encode_application_compression_frame,
    APPLICATION_COMPRESSION_MAGIC,
};

#[test]
fn gzip_text_frame_roundtrips_with_envelope_metadata() {
    let config = ApplicationCompressionConfig {
        enabled: true,
        algorithm: ApplicationCompressionAlgorithm::Gzip,
        threshold_bytes: 16,
    };
    let original = br#"{"type":"message","text":"hello hello hello hello hello hello"}"#;

    let encoded = encode_application_compression_frame(original, false, &config)
        .expect("gzip compression should succeed")
        .expect("payload should be compressed");

    assert!(encoded.starts_with(APPLICATION_COMPRESSION_MAGIC));

    let decoded = decode_application_compression_frame(&encoded, 1024)
        .expect("gzip decompression should succeed")
        .expect("frame should be recognized");

    assert_eq!(decoded.data, original);
    assert!(!decoded.is_binary);
}

#[test]
fn zstd_binary_frame_roundtrips_with_original_binary_kind() {
    let config = ApplicationCompressionConfig {
        enabled: true,
        algorithm: ApplicationCompressionAlgorithm::Zstd,
        threshold_bytes: 16,
    };
    let original = vec![42u8; 4096];

    let encoded = encode_application_compression_frame(&original, true, &config)
        .expect("zstd compression should succeed")
        .expect("payload should be compressed");

    let decoded = decode_application_compression_frame(&encoded, 8192)
        .expect("zstd decompression should succeed")
        .expect("frame should be recognized");

    assert_eq!(decoded.data, original);
    assert!(decoded.is_binary);
}

#[test]
fn small_payload_below_threshold_is_left_uncompressed() {
    let config = ApplicationCompressionConfig {
        enabled: true,
        algorithm: ApplicationCompressionAlgorithm::Gzip,
        threshold_bytes: 1024,
    };

    let encoded = encode_application_compression_frame(b"short", false, &config)
        .expect("compression check should succeed");

    assert!(encoded.is_none());
}

#[test]
fn non_enveloped_binary_payload_is_left_as_raw_message() {
    let decoded = decode_application_compression_frame(b"plain binary", 1024)
        .expect("raw binary should not fail");

    assert!(decoded.is_none());
}
