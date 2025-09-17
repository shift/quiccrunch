use criterion::{black_box, criterion_group, criterion_main, Criterion};
use quiccrunch_core::crypto_accel::{CryptoAccelConfig, HardwareCrypto};

fn bench_crypto_operations(c: &mut Criterion) {
    let config = CryptoAccelConfig::default();
    let crypto = HardwareCrypto::new(config);

    let test_data = b"Hello, World! This is a test message for crypto acceleration benchmarking.";
    let key = b"0123456789abcdef0123456789abcdef"; // 32 bytes for AES-256

    let mut group = c.benchmark_group("crypto_accel");

    group.bench_function("single_encrypt", |b| {
        b.iter(|| {
            let _result = black_box(crypto.encrypt(test_data, key).unwrap());
        });
    });

    group.bench_function("single_decrypt", |b| {
        let encrypted = crypto.encrypt(test_data, key).unwrap();
        b.iter(|| {
            let _result = black_box(crypto.decrypt(&encrypted, key).unwrap());
        });
    });

    // Batch operations
    let batch_data: Vec<&[u8]> = vec![test_data; 10];
    group.bench_function("batch_encrypt_10", |b| {
        b.iter(|| {
            let _result = black_box(crypto.encrypt_batch(&batch_data, key).unwrap());
        });
    });

    group.bench_function("batch_decrypt_10", |b| {
        let encrypted = crypto.encrypt_batch(&batch_data, key).unwrap();
        let encrypted_refs: Vec<&[u8]> = encrypted.iter().map(|v| v.as_slice()).collect();
        b.iter(|| {
            let _result = black_box(crypto.decrypt_batch(&encrypted_refs, key).unwrap());
        });
    });

    group.finish();
}

fn bench_hardware_capabilities(c: &mut Criterion) {
    let mut group = c.benchmark_group("hardware_detection");

    group.bench_function("detect_capabilities", |b| {
        b.iter(|| {
            let _caps = black_box(quiccrunch_core::crypto_accel::HardwareCapabilities::detect());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_crypto_operations,
    bench_hardware_capabilities
);
criterion_main!(benches);
