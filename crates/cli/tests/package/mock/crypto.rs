const INITIAL: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

const ROUND: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

pub fn digest(bytes: &[u8]) -> [u8; 32] {
    let mut padded = bytes.to_vec();
    padded.push(0x80);
    while !(padded.len() + 8).is_multiple_of(64) {
        padded.push(0);
    }
    let bits = (bytes.len() as u64) * 8;
    padded.extend_from_slice(&bits.to_be_bytes());
    let mut state = INITIAL;
    for chunk in padded.chunks_exact(64) {
        compress(&mut state, chunk);
    }
    let mut output = [0_u8; 32];
    for (index, word) in state.iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    output
}

fn compress(state: &mut [u32; 8], chunk: &[u8]) {
    let mut schedule = [0_u32; 64];
    for index in 0..16 {
        let slice = &chunk[index * 4..index * 4 + 4];
        schedule[index] = u32::from_be_bytes(slice.try_into().expect("word"));
    }
    for index in 16..64 {
        let left = schedule[index - 15];
        let right = schedule[index - 2];
        let first = left.rotate_right(7) ^ left.rotate_right(18) ^ (left >> 3);
        let second = right.rotate_right(17) ^ right.rotate_right(19) ^ (right >> 10);
        schedule[index] = schedule[index - 16]
            .wrapping_add(first)
            .wrapping_add(schedule[index - 7])
            .wrapping_add(second);
    }
    let mut work = *state;
    for index in 0..64 {
        step(&mut work, schedule[index], ROUND[index]);
    }
    for index in 0..8 {
        state[index] = state[index].wrapping_add(work[index]);
    }
}

fn step(work: &mut [u32; 8], word: u32, constant: u32) {
    let upper = work[4].rotate_right(6) ^ work[4].rotate_right(11) ^ work[4].rotate_right(25);
    let choice = (work[4] & work[5]) ^ ((!work[4]) & work[6]);
    let carry = work[7]
        .wrapping_add(upper)
        .wrapping_add(choice)
        .wrapping_add(constant)
        .wrapping_add(word);
    let lower = work[0].rotate_right(2) ^ work[0].rotate_right(13) ^ work[0].rotate_right(22);
    let majority = (work[0] & work[1]) ^ (work[0] & work[2]) ^ (work[1] & work[2]);
    let shift = lower.wrapping_add(majority);
    *work = [
        carry.wrapping_add(shift),
        work[0],
        work[1],
        work[2],
        work[3].wrapping_add(carry),
        work[4],
        work[5],
        work[6],
    ];
}

pub fn render(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[(byte >> 4) as usize]));
        text.push(char::from(DIGITS[(byte & 15) as usize]));
    }
    text
}

pub fn authenticate(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0_u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Vec::with_capacity(64 + message.len());
    for byte in &block {
        inner.push(byte ^ 0x36);
    }
    inner.extend_from_slice(message);
    let middle = digest(&inner);
    let mut outer = Vec::with_capacity(96);
    for byte in &block {
        outer.push(byte ^ 0x5c);
    }
    outer.extend_from_slice(&middle);
    digest(&outer)
}

pub fn derive(secret: &str, date: &str, region: &str) -> [u8; 32] {
    let seed = format!("AWS4{secret}");
    let dated = authenticate(seed.as_bytes(), date.as_bytes());
    let regional = authenticate(&dated, region.as_bytes());
    let service = authenticate(&regional, b"s3");
    authenticate(&service, b"aws4_request")
}
