//! Deterministic hashing of positions into pseudo-random numbers.

/// Returns a value in `[0, 1)` that depends only on the arguments.
///
/// `stream` is an ID that is neither a secret nor used for cryptography: it
/// separates independent uses of the same `index`, for example the curvature
/// and the grade of the same road segment.
pub fn hash01(seed: u64, stream: u32, index: i64) -> f32 {
    let mut z = seed
        ^ u64::from(stream).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (index as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93);
    // Two rounds of multiply and xor-shift avalanche every input bit.
    z = (z ^ (z >> 32)).wrapping_mul(0xD6E8_FEB8_6659_FD93);
    z = (z ^ (z >> 32)).wrapping_mul(0xD6E8_FEB8_6659_FD93);
    z ^= z >> 32;
    // 24 bits fit exactly in an f32 mantissa, so the result is below 1.0.
    (z >> 40) as f32 / (1u32 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_input_gives_same_value() {
        assert_eq!(hash01(7, 1, 42), hash01(7, 1, 42));
    }

    #[test]
    fn values_are_in_unit_range() {
        for index in -2000..2000 {
            let value = hash01(123, 3, index);
            assert!((0.0..1.0).contains(&value), "{value}");
        }
    }

    #[test]
    fn seed_stream_and_index_change_the_sequence() {
        let sequence = |seed, stream| (0..16).map(|i| hash01(seed, stream, i)).collect::<Vec<_>>();
        assert_ne!(sequence(1, 0), sequence(2, 0));
        assert_ne!(sequence(1, 0), sequence(1, 1));
    }

    #[test]
    fn values_are_spread_over_the_range() {
        let mean = (0..10_000).map(|i| hash01(5, 0, i)).sum::<f32>() / 10_000.0;
        assert!((mean - 0.5).abs() < 0.02, "{mean}");
    }
}
