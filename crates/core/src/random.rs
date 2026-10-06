//! One hardware seed per generator lifetime, then a continuous ChaCha20 stream.
use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};
use zeroize::Zeroize;

const RETRIES: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Unsupported,
    Unavailable,
    Busy,
}

pub struct Generator {
    rng: Option<ChaCha20Rng>,
}

impl Default for Generator {
    fn default() -> Self {
        Self::new()
    }
}

impl Generator {
    pub const fn new() -> Self {
        Self { rng: None }
    }

    pub fn fill(&mut self, bytes: &mut [u8]) -> Result<(), Error> {
        self.fill_with(bytes, rdrand)
    }

    fn fill_with(
        &mut self,
        bytes: &mut [u8],
        mut source: impl FnMut() -> Result<Option<u64>, Error>,
    ) -> Result<(), Error> {
        if bytes.is_empty() {
            return Ok(());
        }
        if self.rng.is_none() {
            let mut seed = [0; 32];
            let result = (|| {
                for word in seed.chunks_exact_mut(8) {
                    let mut value = None;
                    for _ in 0..RETRIES {
                        value = source()?;
                        if value.is_some() {
                            break;
                        }
                    }
                    word.copy_from_slice(&value.ok_or(Error::Unavailable)?.to_le_bytes());
                }
                self.rng = Some(ChaCha20Rng::from_seed(seed));
                Ok(())
            })();
            seed.zeroize();
            result?;
        }
        self.rng
            .as_mut()
            .expect("Seeded generator")
            .fill_bytes(bytes);
        Ok(())
    }
}

#[cfg(target_arch = "x86_64")]
fn rdrand() -> Result<Option<u64>, Error> {
    // SAFETY: CPUID is available on x86-64. Its feature bit gates RDRAND.
    if unsafe { core::arch::x86_64::__cpuid(1) }.ecx & (1 << 30) == 0 {
        return Err(Error::Unsupported);
    }
    let mut value = 0;
    // SAFETY: RDRAND support was checked; the intrinsic reports CF success.
    let success = unsafe { core::arch::x86_64::_rdrand64_step(&mut value) };
    Ok((success == 1).then_some(value))
}

#[cfg(not(target_arch = "x86_64"))]
fn rdrand() -> Result<Option<u64>, Error> {
    Err(Error::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn failed_seed_is_bounded_and_can_be_retried() {
        let mut generator = Generator::new();
        let mut output = [0xa5; 7];
        let mut calls = 0;
        assert_eq!(
            generator.fill_with(&mut output, || {
                calls += 1;
                Ok(None)
            }),
            Err(Error::Unavailable)
        );
        assert_eq!(calls, RETRIES);
        assert_eq!(output, [0xa5; 7]);
        for successful_words in 1..4 {
            let mut calls = 0;
            assert_eq!(
                generator.fill_with(&mut output, || {
                    calls += 1;
                    Ok((calls <= successful_words).then_some(19))
                }),
                Err(Error::Unavailable)
            );
            assert_eq!(calls, successful_words + RETRIES);
            assert!(generator.rng.is_none());
            assert_eq!(output, [0xa5; 7]);
        }
        assert_eq!(
            generator.fill_with(&mut output, || Err(Error::Unsupported)),
            Err(Error::Unsupported)
        );
        generator.fill_with(&mut output, || Ok(Some(17))).unwrap();
        assert_ne!(output, [0xa5; 7]);
    }

    #[test]
    fn partitioning_preserves_stream_without_reseeding() {
        let mut whole = Generator::new();
        let mut split = Generator::new();
        let mut a = vec![0; 1024 * 1024 + 73];
        let mut b = vec![0; a.len()];
        let mut word_a = 0;
        whole
            .fill_with(&mut a, || {
                word_a += 1;
                Ok(Some(word_a))
            })
            .unwrap();
        let mut word_b = 0;
        // rand_core may discard bytes at the end of a partial word. Compare
        // word-aligned partitions rather than requiring a byte-stream API.
        for part in b.chunks_mut(136) {
            split
                .fill_with(part, || {
                    word_b += 1;
                    Ok(Some(word_b))
                })
                .unwrap();
        }
        assert!(a == b, "Partitioned output differs");
        assert_eq!(word_a, 4);
        assert_eq!(word_b, 4);
        split
            .fill_with(&mut [0; 17], || panic!("Must not reseed"))
            .unwrap();
    }

    #[test]
    fn retry_boundary_and_chacha20_known_vector() {
        let mut generator = Generator::new();
        let mut calls = 0;
        let mut output = [0; 16];
        generator
            .fill_with(&mut output, || {
                calls += 1;
                Ok((calls % RETRIES == 0).then_some(0))
            })
            .unwrap();
        assert_eq!(calls, 4 * RETRIES);
        // ChaCha20's zero-key, zero-nonce, counter-zero test vector.
        assert_eq!(
            output,
            [
                0x76, 0xb8, 0xe0, 0xad, 0xa0, 0xf1, 0x3d, 0x90, 0x40, 0x5d, 0x6a, 0xe5, 0x53, 0x86,
                0xbd, 0x28
            ]
        );
    }
}
