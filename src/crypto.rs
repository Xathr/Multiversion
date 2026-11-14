//! Login encryption (RSA key exchange, AES key), the Mojang session check, and HMAC signing for
//! the Velocity-style forwarding payload that Pumpkin verifies.

use std::cell::OnceCell;

use hmac::{Hmac, Mac};
use rsa::pkcs8::EncodePublicKey;
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey};
use sha1::{Digest, Sha1};
use sha2::Sha256;

/// The server's RSA key pair for login encryption. Only online-mode logins use it, so it can be
/// left unmade ([`ServerKeys::deferred`]) until one asks for it: making a key is the slowest
/// thing the plugin does when it starts, and an offline-mode server never needs one.
pub struct ServerKeys {
    pair: OnceCell<Result<KeyPair, String>>,
}

struct KeyPair {
    private: RsaPrivateKey,
    /// DER-encoded SubjectPublicKeyInfo, as sent in the encryption request.
    public_der: Vec<u8>,
}

impl KeyPair {
    fn generate() -> Result<Self, String> {
        let mut rng = WasiRng;
        let private = RsaPrivateKey::new(&mut rng, 1024).map_err(|e| format!("rsa keygen: {e}"))?;
        let public = RsaPublicKey::from(&private);
        let public_der = public.to_public_key_der().map_err(|e| e.to_string())?.as_bytes().to_vec();
        Ok(Self { private, public_der })
    }
}

impl ServerKeys {
    /// Makes the key pair now.
    pub fn generate() -> Result<Self, String> {
        let keys = Self::deferred();
        keys.pair()?;
        Ok(keys)
    }
    /// Makes the key pair the first time it is needed.
    pub fn deferred() -> Self {
        Self { pair: OnceCell::new() }
    }
    /// Whether the key pair exists yet.
    pub fn is_made(&self) -> bool {
        self.pair.get().is_some()
    }
    fn pair(&self) -> Result<&KeyPair, String> {
        self.pair.get_or_init(KeyPair::generate).as_ref().map_err(Clone::clone)
    }
    /// DER-encoded SubjectPublicKeyInfo, as sent in the encryption request.
    pub fn public_der(&self) -> Result<&[u8], String> {
        self.pair().map(|pair| &pair.public_der[..])
    }
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        self.pair()?.private.decrypt(Pkcs1v15Encrypt, data).map_err(|e| format!("rsa decrypt: {e}"))
    }
}

/// `rand_core` RNG backed by WASI random (via getrandom's custom backend below).
pub struct WasiRng;

impl rand_core::RngCore for WasiRng {
    fn next_u32(&mut self) -> u32 {
        let mut b = [0u8; 4];
        self.fill_bytes(&mut b);
        u32::from_le_bytes(b)
    }
    fn next_u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        self.fill_bytes(&mut b);
        u64::from_le_bytes(b)
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let bytes = crate::platform::random_bytes(dest.len());
        dest.copy_from_slice(&bytes);
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}
impl rand_core::CryptoRng for WasiRng {}

pub fn random_bytes(n: usize) -> Vec<u8> {
    crate::platform::random_bytes(n)
}

/// URL-safe random token for secrets.
pub fn random_token() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    random_bytes(32).iter().map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char).collect()
}

/// Vanilla's server hash: hex of SHA-1(serverId + sharedSecret + publicKeyDer) as a signed
/// two's-complement big integer.
pub fn server_hash(server_id: &str, shared_secret: &[u8], public_der: &[u8]) -> String {
    let mut h = Sha1::new();
    h.update(server_id.as_bytes());
    h.update(shared_secret);
    h.update(public_der);
    let digest: [u8; 20] = h.finalize().into();
    let negative = digest[0] & 0x80 != 0;
    let mut bytes = digest;
    if negative {
        // two's complement negate
        let mut carry = true;
        for b in bytes.iter_mut().rev() {
            *b = !*b;
            if carry {
                let (v, c) = b.overflowing_add(1);
                *b = v;
                carry = c;
            }
        }
    }
    let hex = hex::encode(bytes);
    let trimmed = hex.trim_start_matches('0');
    let trimmed = if trimmed.is_empty() { "0" } else { trimmed };
    if negative { format!("-{trimmed}") } else { trimmed.to_string() }
}

pub fn hmac_sha256(secret: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret).expect("hmac accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn server_hash_known_vectors() {
        // From the protocol wiki
        assert_eq!(server_hash("Notch", b"", b""), "4ed1f46bbe04bc756bcb17c0c7ce3e4632f06a48");
        assert_eq!(server_hash("jeb_", b"", b""), "-7c9d5b0044c130109a5d7b5fb5c317c02b4e28c1");
        assert_eq!(server_hash("simon", b"", b""), "88e16a1019277b15d58faf0541e11910eb756f6");
    }

    fn encrypt_for(keys: &ServerKeys, data: &[u8]) -> Vec<u8> {
        use rsa::pkcs8::DecodePublicKey;
        let public = RsaPublicKey::from_public_key_der(keys.public_der().unwrap()).unwrap();
        public.encrypt(&mut WasiRng, Pkcs1v15Encrypt, data).unwrap()
    }

    /// What a client encrypts with the public key it was sent, the server can read.
    #[test]
    fn keys_decrypt_what_clients_encrypt() {
        let keys = ServerKeys::generate().unwrap();
        assert!(keys.is_made());
        let secret = [7u8; 16];
        assert_eq!(keys.decrypt(&encrypt_for(&keys, &secret)).unwrap(), secret);
        assert_eq!(keys.decrypt(&encrypt_for(&keys, &[1, 2, 3, 4])).unwrap(), [1, 2, 3, 4]);
        // not something this key encrypted
        assert!(keys.decrypt(&[0u8; 128]).is_err());
        assert!(keys.decrypt(&[1, 2, 3]).is_err());
        // a 1024-bit key, in the form the game reads
        assert_eq!(keys.public_der().unwrap().len(), 162);
        // another server has another key
        let other = ServerKeys::generate().unwrap();
        assert_ne!(other.public_der().unwrap(), keys.public_der().unwrap());
        assert!(other.decrypt(&encrypt_for(&keys, &secret)).map_or(true, |plain| plain != secret));
    }

    /// A deferred key pair is made on first use and is the same one from then on.
    #[test]
    fn deferred_keys_are_made_once_on_first_use() {
        let keys = ServerKeys::deferred();
        assert!(!keys.is_made());
        let der = keys.public_der().unwrap().to_vec();
        assert!(keys.is_made());
        assert_eq!(keys.public_der().unwrap(), der);
        let secret = [9u8; 16];
        assert_eq!(keys.decrypt(&encrypt_for(&keys, &secret)).unwrap(), secret);
        assert_eq!(keys.public_der().unwrap(), der);

        // decrypting is also a first use
        let keys = ServerKeys::deferred();
        assert!(keys.decrypt(&[0u8; 128]).is_err());
        assert!(keys.is_made());
    }
}
