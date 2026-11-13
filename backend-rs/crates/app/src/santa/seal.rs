//! Sealing a pairing while its event is active: XChaCha20-Poly1305 over the receiver's id, with
//! the event, giver and tier as associated data. A sealed value copied to another row (another
//! giver, tier or event) no longer opens, so it cannot be used to read someone else's draw.
//!
//! Stored as `nonce (24 bytes) || ciphertext`, with a fresh random nonce per pairing.

use std::fmt;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore as _;

const NONCE_LEN: usize = 24;
/// Bumped if the format ever changes, so old seals fail to open instead of misreading.
const CONTEXT: &[u8] = b"stowarzysz/santa/v1";

/// The seal could not be opened: wrong key, or a row that is not what it claims to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("unreadable Secret Santa pairing")]
pub struct Unreadable;

pub struct Sealer {
    cipher: XChaCha20Poly1305,
}

impl fmt::Debug for Sealer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sealer(***)")
    }
}

fn associated(event_id: i64, giver_id: i64, tier: Option<i16>) -> Vec<u8> {
    let mut data = CONTEXT.to_vec();
    data.extend_from_slice(&event_id.to_be_bytes());
    data.extend_from_slice(&giver_id.to_be_bytes());
    data.extend_from_slice(&tier.unwrap_or(-1).to_be_bytes());
    data
}

impl Sealer {
    pub fn new(key: &[u8; 32]) -> Self {
        Self {
            cipher: XChaCha20Poly1305::new(key.into()),
        }
    }

    /// Seal "`giver_id` gives to `receiver_id`" for one tier (`None`: every tier) of an event.
    pub fn seal(
        &self,
        event_id: i64,
        giver_id: i64,
        tier: Option<i16>,
        receiver_id: i64,
    ) -> Vec<u8> {
        let mut nonce = [0u8; NONCE_LEN];
        rand::rng().fill_bytes(&mut nonce);
        let aad = associated(event_id, giver_id, tier);
        let ciphertext = self
            .cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &receiver_id.to_be_bytes(),
                    aad: &aad,
                },
            )
            .unwrap_or_default(); // only fails for plaintexts beyond 256 GiB
        let mut sealed = nonce.to_vec();
        sealed.extend_from_slice(&ciphertext);
        sealed
    }

    /// The receiver sealed for this event, giver and tier.
    pub fn open(
        &self,
        event_id: i64,
        giver_id: i64,
        tier: Option<i16>,
        sealed: &[u8],
    ) -> Result<i64, Unreadable> {
        if sealed.len() <= NONCE_LEN {
            return Err(Unreadable);
        }
        let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
        let aad = associated(event_id, giver_id, tier);
        let plain = self
            .cipher
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| Unreadable)?;
        let bytes: [u8; 8] = plain.try_into().map_err(|_| Unreadable)?;
        Ok(i64::from_be_bytes(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seal_opens_only_for_its_own_event_giver_and_tier() {
        let keys = Sealer::new(&[7; 32]);
        let sealed = keys.seal(1, 10, Some(0), 42);
        assert_eq!(keys.open(1, 10, Some(0), &sealed), Ok(42));
        assert_eq!(keys.open(1, 11, Some(0), &sealed), Err(Unreadable));
        assert_eq!(keys.open(2, 10, Some(0), &sealed), Err(Unreadable));
        assert_eq!(keys.open(1, 10, None, &sealed), Err(Unreadable));
        assert_eq!(keys.open(1, 10, Some(1), &sealed), Err(Unreadable));
        assert_eq!(
            Sealer::new(&[8; 32]).open(1, 10, Some(0), &sealed),
            Err(Unreadable)
        );
        let mut tampered = sealed.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert_eq!(keys.open(1, 10, Some(0), &tampered), Err(Unreadable));
        assert_eq!(keys.open(1, 10, Some(0), &sealed[..10]), Err(Unreadable));
    }

    #[test]
    fn the_same_pairing_seals_differently_each_time() {
        let keys = Sealer::new(&[7; 32]);
        assert_ne!(keys.seal(1, 10, None, 42), keys.seal(1, 10, None, 42));
        assert!(
            !keys
                .seal(1, 10, None, 42)
                .windows(8)
                .any(|w| w == 42_i64.to_be_bytes())
        );
    }
}
