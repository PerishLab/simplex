use super::format::PAGE;
use crate::digest::compute as hash;

const SHIFT: u8 = 14;
const FIXED: u32 = 88;

pub fn measure(limit: usize, name: &str) -> usize {
    let label = identifier(name);
    let slots = limit.div_ceil(PAGE as usize);
    let directory = (FIXED as usize) + label.len() + 1 + slots * 32;
    20 + directory
}

pub fn sign(prefix: &[u8], name: &str, text: u64, executable: bool) -> Vec<u8> {
    let label = identifier(name);
    let limit = prefix.len();
    let step = PAGE as usize;
    let slots = limit.div_ceil(step);
    let title = FIXED;
    let hashes = title + (label.len() as u32) + 1;
    let body = hashes + (slots as u32) * 32;
    let envelope = 20 + body;
    let flags: u32 = 0x0002_0002;
    let markers: u64 = if executable { 1 } else { 0 };

    let mut out = Vec::with_capacity(envelope as usize);
    out.extend_from_slice(&0xFADE_0CC0u32.to_be_bytes());
    out.extend_from_slice(&envelope.to_be_bytes());
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&20u32.to_be_bytes());

    out.extend_from_slice(&0xFADE_0C02u32.to_be_bytes());
    out.extend_from_slice(&body.to_be_bytes());
    out.extend_from_slice(&0x0002_0400u32.to_be_bytes());
    out.extend_from_slice(&flags.to_be_bytes());
    out.extend_from_slice(&hashes.to_be_bytes());
    out.extend_from_slice(&title.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&(slots as u32).to_be_bytes());
    out.extend_from_slice(&(limit as u32).to_be_bytes());
    out.extend_from_slice(&[32u8, 2u8, 0u8, SHIFT]);
    out.extend_from_slice(&[0u8; 16]);
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&text.to_be_bytes());
    out.extend_from_slice(&markers.to_be_bytes());

    out.extend_from_slice(label.as_bytes());
    out.push(0);

    for index in 0..slots {
        let start = index * step;
        let finish = ((index + 1) * step).min(limit);
        let digest = hash(&prefix[start..finish]);
        out.extend_from_slice(&digest);
    }
    out
}

fn identifier(name: &str) -> &str {
    match name.is_empty() {
        true => "simplex",
        false => name,
    }
}
