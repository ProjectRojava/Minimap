//! Encryption for everything that goes to Google Drive and for cached attachments (spec 22).
//!
//! * **Attachments** are encrypted in 64 KiB chunks with XChaCha20-Poly1305. Every chunk has its
//!   own nonce (a random 16-byte prefix per file plus the chunk number), and the last chunk is
//!   marked in the authenticated data, so reordering, dropping or truncating chunks is detected.
//! * **Small values** (the key check in `vault.json`) are sealed with a random nonce.
//! * **Snapshots** are SQLCipher databases keyed with the vault key (see `minimap-store`).
//!
//! Subkeys are derived from the vault key by hashing it with a purpose label, so the key used
//! for attachments is not the key used for snapshots.

use std::io::{Read, Write};

use chacha20poly1305::{
    aead::{Aead, Payload},
    KeyInit, XChaCha20Poly1305, XNonce,
};
use minimap_store::security::RawKey;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::error::{Result, SyncError};

const MAGIC: &[u8; 4] = b"MMB1";
const PREFIX_LEN: usize = 16;
/// Plaintext bytes per chunk.
pub const CHUNK: usize = 64 * 1024;
const TAG: usize = 16;

fn fill_random(buf: &mut [u8]) -> Result<()> {
    getrandom::fill(buf).map_err(|e| SyncError::Io(format!("no secure random source: {e}")))
}

/// A subkey for one purpose.
fn derive(vault: &RawKey, purpose: &str) -> Zeroizing<[u8; 32]> {
    let mut hasher = Sha256::new();
    hasher.update(b"minimap/subkey/v1\0");
    hasher.update(purpose.as_bytes());
    hasher.update([0u8]);
    hasher.update(vault.expose());
    let digest = hasher.finalize();
    let mut out = Zeroizing::new([0u8; 32]);
    out.copy_from_slice(&digest);
    out
}

fn cipher(vault: &RawKey, purpose: &str) -> XChaCha20Poly1305 {
    let key = derive(vault, purpose);
    // A 32-byte slice is always a valid key length, so this cannot fail.
    XChaCha20Poly1305::new((&*key).into())
}

fn chunk_nonce(prefix: &[u8; PREFIX_LEN], index: u64) -> XNonce {
    let mut n = [0u8; 24];
    n[..PREFIX_LEN].copy_from_slice(prefix);
    n[PREFIX_LEN..].copy_from_slice(&index.to_be_bytes());
    XNonce::from(n)
}

/// Reads until `buf` is full or the input ends; returns how many bytes it got.
fn read_full(r: &mut impl Read, buf: &mut [u8]) -> Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        match r.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(SyncError::io("Couldn't read a file", e)),
        }
    }
    Ok(got)
}

fn write_all(w: &mut impl Write, bytes: &[u8]) -> Result<()> {
    w.write_all(bytes)
        .map_err(|e| SyncError::io("Couldn't write a file", e))
}

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// What an encryption produced: the plaintext's size and SHA-256 (hex), which is the name the
/// file is stored under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sealed {
    pub plain_bytes: u64,
    pub sha256: String,
}

/// Encrypts everything `input` yields into `output`.
pub fn encrypt_stream(
    vault: &RawKey,
    input: &mut impl Read,
    output: &mut impl Write,
) -> Result<Sealed> {
    let aead = cipher(vault, "media");
    let mut prefix = [0u8; PREFIX_LEN];
    fill_random(&mut prefix)?;
    write_all(output, MAGIC)?;
    write_all(output, &prefix)?;

    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut current = vec![0u8; CHUNK];
    let mut next = vec![0u8; CHUNK];
    let mut current_len = read_full(input, &mut current)?;
    let mut index = 0u64;
    loop {
        let next_len = if current_len == CHUNK {
            read_full(input, &mut next)?
        } else {
            0
        };
        let last = next_len == 0;
        let plain = &current[..current_len];
        hasher.update(plain);
        total += plain.len() as u64;
        let sealed = aead
            .encrypt(
                &chunk_nonce(&prefix, index),
                Payload {
                    msg: plain,
                    aad: &[u8::from(last)],
                },
            )
            .map_err(|_| SyncError::Corrupt("encryption failed".into()))?;
        write_all(output, &sealed)?;
        if last {
            break;
        }
        std::mem::swap(&mut current, &mut next);
        current_len = next_len;
        index += 1;
    }
    Ok(Sealed {
        plain_bytes: total,
        sha256: hex(&hasher.finalize()),
    })
}

/// Decrypts what [`encrypt_stream`] wrote. A wrong key, a changed chunk, a missing or reordered
/// chunk or a cut-off end all fail with `Corrupt`. When `expect_sha256` is given, the plaintext
/// must hash to it.
pub fn decrypt_stream(
    vault: &RawKey,
    input: &mut impl Read,
    output: &mut impl Write,
    expect_sha256: Option<&str>,
) -> Result<Sealed> {
    let aead = cipher(vault, "media");
    let mut header = [0u8; 4 + PREFIX_LEN];
    if read_full(input, &mut header)? != header.len() || &header[..4] != MAGIC {
        return Err(SyncError::Corrupt("not an encrypted attachment".into()));
    }
    let mut prefix = [0u8; PREFIX_LEN];
    prefix.copy_from_slice(&header[4..]);

    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let size = CHUNK + TAG;
    let mut current = vec![0u8; size];
    let mut next = vec![0u8; size];
    let mut current_len = read_full(input, &mut current)?;
    let mut index = 0u64;
    loop {
        if current_len < TAG {
            return Err(SyncError::Corrupt("the file is cut short".into()));
        }
        let next_len = if current_len == size {
            read_full(input, &mut next)?
        } else {
            0
        };
        let last = next_len == 0;
        let plain = aead
            .decrypt(
                &chunk_nonce(&prefix, index),
                Payload {
                    msg: &current[..current_len],
                    aad: &[u8::from(last)],
                },
            )
            .map_err(|_| {
                SyncError::Corrupt("a chunk failed its check (wrong key or changed data)".into())
            })?;
        hasher.update(&plain);
        total += plain.len() as u64;
        write_all(output, &plain)?;
        if last {
            break;
        }
        std::mem::swap(&mut current, &mut next);
        current_len = next_len;
        index += 1;
    }
    let sha256 = hex(&hasher.finalize());
    if let Some(expected) = expect_sha256 {
        if sha256 != expected {
            return Err(SyncError::Corrupt(
                "the content doesn't match its checksum".into(),
            ));
        }
    }
    Ok(Sealed {
        plain_bytes: total,
        sha256,
    })
}

/// SHA-256 (hex) and size of a file's content, without encrypting.
pub fn hash_reader(input: &mut impl Read) -> Result<Sealed> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut total = 0u64;
    loop {
        let n = read_full(input, &mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n as u64;
        if n < CHUNK {
            break;
        }
    }
    Ok(Sealed {
        plain_bytes: total,
        sha256: hex(&hasher.finalize()),
    })
}

/// Seals a small value: a random nonce, then the ciphertext, bound to `purpose`.
pub fn seal(vault: &RawKey, purpose: &str, plain: &[u8]) -> Result<Vec<u8>> {
    let aead = cipher(vault, purpose);
    let mut nonce = [0u8; 24];
    fill_random(&mut nonce)?;
    let sealed = aead
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: plain,
                aad: purpose.as_bytes(),
            },
        )
        .map_err(|_| SyncError::Corrupt("encryption failed".into()))?;
    let mut out = nonce.to_vec();
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// Opens what [`seal`] made; `WrongKey` when the key or purpose doesn't match.
pub fn open(vault: &RawKey, purpose: &str, sealed: &[u8]) -> Result<Vec<u8>> {
    if sealed.len() < 24 + TAG {
        return Err(SyncError::Corrupt("a sealed value is cut short".into()));
    }
    let (nonce, body) = sealed.split_at(24);
    let mut n = [0u8; 24];
    n.copy_from_slice(nonce);
    cipher(vault, purpose)
        .decrypt(
            &XNonce::from(n),
            Payload {
                msg: body,
                aad: purpose.as_bytes(),
            },
        )
        .map_err(|_| SyncError::WrongKey)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> RawKey {
        RawKey::generate().unwrap()
    }

    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    fn encrypt(k: &RawKey, plain: &[u8]) -> (Vec<u8>, Sealed) {
        let mut out = Vec::new();
        let sealed = encrypt_stream(k, &mut &plain[..], &mut out).unwrap();
        (out, sealed)
    }

    fn decrypt(k: &RawKey, cipher: &[u8]) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        decrypt_stream(k, &mut &cipher[..], &mut out, None)?;
        Ok(out)
    }

    #[test]
    fn files_of_every_awkward_size_round_trip() {
        let k = key();
        for len in [
            0,
            1,
            15,
            CHUNK - 1,
            CHUNK,
            CHUNK + 1,
            2 * CHUNK,
            3 * CHUNK + 5,
        ] {
            let plain = pattern(len);
            let (cipher, sealed) = encrypt(&k, &plain);
            assert_eq!(sealed.plain_bytes, len as u64);
            assert_eq!(decrypt(&k, &cipher).unwrap(), plain, "len {len}");
            // The checksum is the plaintext's SHA-256.
            let hashed = hash_reader(&mut &plain[..]).unwrap();
            assert_eq!(hashed, sealed, "len {len}");
        }
    }

    #[test]
    fn the_sha256_is_the_standard_one() {
        let (_, sealed) = encrypt(&key(), b"abc");
        assert_eq!(
            sealed.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn nothing_readable_is_left_and_every_encryption_differs() {
        let k = key();
        let plain = b"confidential budget numbers, long enough to spot".repeat(10);
        let (a, _) = encrypt(&k, &plain);
        let (b, _) = encrypt(&k, &plain);
        assert_ne!(a, b, "a fresh nonce prefix every time");
        assert!(!a.windows(12).any(|w| w == b"confidential"));
        assert_eq!(a.len(), 4 + PREFIX_LEN + plain.len() + TAG);
    }

    #[test]
    fn a_wrong_key_changed_byte_or_swapped_header_is_refused() {
        let k = key();
        let plain = pattern(3 * CHUNK + 10);
        let (cipher, _) = encrypt(&k, &plain);
        assert!(matches!(
            decrypt(&key(), &cipher),
            Err(SyncError::Corrupt(_))
        ));
        for at in [0, 5, 4 + PREFIX_LEN, cipher.len() / 2, cipher.len() - 1] {
            let mut bad = cipher.clone();
            bad[at] ^= 1;
            assert!(decrypt(&k, &bad).is_err(), "byte {at}");
        }
        assert!(decrypt(&k, b"").is_err());
        assert!(decrypt(&k, b"MMB1").is_err());
    }

    #[test]
    fn dropped_truncated_or_reordered_chunks_are_detected() {
        let k = key();
        let plain = pattern(3 * CHUNK + 10);
        let (cipher, _) = encrypt(&k, &plain);
        let body = 4 + PREFIX_LEN;
        let size = CHUNK + TAG;
        // Cut at a chunk boundary: the new "last" chunk was not sealed as last.
        assert!(decrypt(&k, &cipher[..body + 2 * size]).is_err());
        assert!(decrypt(&k, &cipher[..body + size]).is_err());
        // Cut inside a chunk.
        assert!(decrypt(&k, &cipher[..cipher.len() - 5]).is_err());
        // Two chunks swapped.
        let mut swapped = cipher[..body].to_vec();
        swapped.extend_from_slice(&cipher[body + size..body + 2 * size]);
        swapped.extend_from_slice(&cipher[body..body + size]);
        swapped.extend_from_slice(&cipher[body + 2 * size..]);
        assert!(decrypt(&k, &swapped).is_err());
        // Extra data appended after the end.
        let mut longer = cipher.clone();
        longer.extend_from_slice(&[0u8; 40]);
        assert!(decrypt(&k, &longer).is_err());
    }

    #[test]
    fn a_checksum_that_does_not_match_is_refused() {
        let k = key();
        let (cipher, sealed) = encrypt(&k, b"hello");
        let mut out = Vec::new();
        assert!(decrypt_stream(&k, &mut &cipher[..], &mut out, Some(&sealed.sha256)).is_ok());
        let wrong = "0".repeat(64);
        assert!(matches!(
            decrypt_stream(&k, &mut &cipher[..], &mut Vec::new(), Some(&wrong)),
            Err(SyncError::Corrupt(_))
        ));
    }

    #[test]
    fn small_values_are_bound_to_their_purpose() {
        let k = key();
        let sealed = seal(&k, "vault-check", b"ok").unwrap();
        assert_eq!(open(&k, "vault-check", &sealed).unwrap(), b"ok");
        assert!(matches!(
            open(&key(), "vault-check", &sealed),
            Err(SyncError::WrongKey)
        ));
        assert!(matches!(
            open(&k, "other", &sealed),
            Err(SyncError::WrongKey)
        ));
        assert!(open(&k, "vault-check", &sealed[..10]).is_err());
        assert_ne!(sealed, seal(&k, "vault-check", b"ok").unwrap());
    }

    #[test]
    fn subkeys_differ_by_purpose() {
        let k = key();
        assert_ne!(*derive(&k, "media"), *derive(&k, "vault-check"));
        assert_ne!(*derive(&k, "media"), *k.expose());
    }
}
