use byteorder::{ByteOrder, LittleEndian};
use ring::aead;
use ring::digest;
use ring::error::Unspecified;
use ring::rand::{SystemRandom, SecureRandom};
use std;

// Authentication utilities for a single entry
pub fn generate_key(target: i32, master: &[u8]) -> Result<String, Unspecified> {
    // Convert to bytes
    let mut buf: [u8; 4 + aead::MAX_TAG_LEN] = [0; 4 + aead::MAX_TAG_LEN]; // 4 bytes for input, MAX_LAG_LEN for cap
    LittleEndian::write_i32(&mut buf, target);

    // Create a 256-bit hash of the master secret
    let hash = digest::digest(&digest::SHA256, master);

    // Creating sealing key
    let sealing_key = aead::SealingKey::new(&aead::AES_256_GCM, hash.as_ref())?;

    // Generate a 96-bit nonce
    let mut nonce: [u8; 12] = [0; 12];
    let rng = SystemRandom::new();
    rng.fill(&mut nonce)?;

    let len = aead::seal_in_place(
        &sealing_key,
        &nonce,
        &[],
        &mut buf,
        aead::MAX_TAG_LEN)?;
    let mut result = String::with_capacity(12 * 2 + len * 2);

    for byte in &nonce {
        write!(&mut result as &mut std::fmt::Write, "{:02x}", byte).map_err(|_| Unspecified)?
    }
    for byte in &buf[..len] {
        write!(&mut result as &mut std::fmt::Write, "{:02x}", byte).map_err(|_| Unspecified)?
    }
    Ok(result)
}

pub fn try_decrypt_key(key: &str, master: &[u8]) -> Option<i32> {
    if key.len() % 2 != 0 || key.len() < 12 * 2 {
        // Got no nounce
        return None;
    }

    let data = &key[12*2..];
    let mut nonce_vec: [u8; 12] = [0; 12];
    let mut data_vec: Vec<u8> = Vec::with_capacity(data.len() / 2);

    let result: Result<(), std::num::ParseIntError> = do catch {
        for i in 0..12 {
            nonce_vec[i] = u8::from_str_radix(&key[i*2..i*2+2], 16)?;
        }

        for i in 0..data.len() / 2 {
            data_vec.push(u8::from_str_radix(&data[i*2..i*2+2], 16)?);
        }
        Ok(())
    };

    if result.is_err() {
        return None;
    }

    let result: Result<(), Unspecified> = do catch {
        // Create a 256-bit hash of the master secret
        let hash = digest::digest(&digest::SHA256, master);

        // Creating opening key
        let opening_key = aead::OpeningKey::new(&aead::AES_256_GCM, hash.as_ref())?;


        aead::open_in_place(
            &opening_key,
            &nonce_vec,
            &[],
            0,
            &mut data_vec)?;
        Ok(())
    };

    if result.is_err() {
        return None;
    }

    Some(LittleEndian::read_i32(data_vec.as_slice()))
}
