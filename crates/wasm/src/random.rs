//! Cryptographic random bytes come from a host provider.

/// Bound synchronous allocation and generation in one guest call.
pub const MAX_RANDOM_BYTES: usize = 512 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandomError {
    Unavailable,
    Io,
}

impl RandomError {
    pub(crate) fn errno(self) -> i32 {
        match self {
            Self::Unavailable => 58,
            Self::Io => 29,
        }
    }
}

/// Fill the entire buffer with cryptographic random bytes on success.
/// Providers can retain a generator shared by the world's processes. They
/// must not restart the same seeded stream for each process or request.
pub trait Random {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), RandomError>;
}
