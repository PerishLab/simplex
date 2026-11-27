use sha2::{Digest, Sha256};

pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let high = char::from_digit((byte >> 4) as u32, 16).unwrap_or('0');
        let low = char::from_digit((byte & 0x0f) as u32, 16).unwrap_or('0');
        out.push(high);
        out.push(low);
    }
    out
}

pub(crate) fn hash(payload: &[u8]) -> String {
    let digest = Sha256::digest(payload);
    hex(&digest)
}

pub(crate) fn mac(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        let shortened = Sha256::digest(key);
        block[..32].copy_from_slice(&shortened);
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = [0x36u8; 64];
    let mut outer = [0x5cu8; 64];
    for index in 0..64 {
        inner[index] ^= block[index];
        outer[index] ^= block[index];
    }
    let mut first = Sha256::new();
    first.update(inner);
    first.update(message);
    let mid = first.finalize();

    let mut second = Sha256::new();
    second.update(outer);
    second.update(mid);
    second.finalize().into()
}

pub(crate) fn derive(secret: &str, date: &str, region: &str) -> [u8; 32] {
    let seed = format!("AWS4{secret}");
    let daily = mac(seed.as_bytes(), date.as_bytes());
    let regional = mac(&daily, region.as_bytes());
    let service = mac(&regional, b"s3");
    mac(&service, b"aws4_request")
}

#[cfg(test)]
mod tests {
    use super::{hex, mac};

    #[test]
    fn rfc() {
        let short = mac(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex(&short),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        let large = mac(
            &[0xaa; 131],
            b"Test Using Larger Than Block-Size Key - Hash Key First",
        );
        assert_eq!(
            hex(&large),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }
}
