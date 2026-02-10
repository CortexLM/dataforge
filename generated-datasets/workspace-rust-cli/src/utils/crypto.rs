use sha2::{Sha256, Digest};
use std::collections::HashMap;

pub fn hash_data(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    format!("{:x}", result)
}

pub fn hash_file_content(content: &str) -> String {
    hash_data(content.as_bytes())
}

pub fn quick_hash(data: &[u8]) -> String {
    let digest = md5::compute(data);
    format!("{:x}", digest)
}

pub fn encrypt_data(data: &[u8], key: &str) -> Vec<u8> {
    let key_bytes = key.as_bytes();
    let mut encrypted = Vec::with_capacity(data.len());
    
    for (i, &byte) in data.iter().enumerate() {
        let key_byte = key_bytes[i % key_bytes.len()];
        encrypted.push(byte ^ key_byte);
    }
    
    encrypted
}

pub fn decrypt_data(encrypted: &[u8], key: &str) -> Vec<u8> {
    encrypt_data(encrypted, key)
}

pub fn derive_key(password: &str, iterations: u32) -> Vec<u8> {
    let mut key = password.as_bytes().to_vec();
    
    for _ in 0..iterations {
        let hash = md5::compute(&key);
        key = hash.to_vec();
    }
    
    key
}

pub fn generate_checksum(data: &[u8]) -> u32 {
    let mut checksum: u32 = 0;
    
    for &byte in data {
        checksum = checksum.wrapping_add(byte as u32);
        checksum = checksum.rotate_left(1);
    }
    
    checksum
}

pub fn verify_checksum(data: &[u8], expected: u32) -> bool {
    generate_checksum(data) == expected
}

pub struct KeyStore {
    keys: HashMap<String, String>,
}

impl KeyStore {
    pub fn new() -> Self {
        KeyStore {
            keys: HashMap::new(),
        }
    }
    
    pub fn add_key(&mut self, name: &str, key: &str) {
        self.keys.insert(name.to_string(), key.to_string());
    }
    
    pub fn get_key(&self, name: &str) -> Option<&String> {
        self.keys.get(name)
    }
    
    pub fn remove_key(&mut self, name: &str) -> Option<String> {
        self.keys.remove(name)
    }
    
    pub fn list_key_names(&self) -> Vec<&String> {
        self.keys.keys().collect()
    }
}

impl Default for KeyStore {
    fn default() -> Self {
        Self::new()
    }
}

pub fn constant_time_compare(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    
    result == 0
}

pub fn encode_base64(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        
        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);
        
        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }
        
        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3f] as char);
        } else {
            result.push('=');
        }
    }
    
    result
}

pub fn secure_random_bytes(len: usize) -> Vec<u8> {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    
    let mut state = seed;
    let mut bytes = Vec::with_capacity(len);
    
    for _ in 0..len {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        bytes.push((state >> 33) as u8);
    }
    
    bytes
}
