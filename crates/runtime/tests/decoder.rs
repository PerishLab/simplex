use simplex_runtime::{Hook, Stub, stubs};
use std::collections::BTreeSet;

struct Analysis {
    boundaries: BTreeSet<usize>,
    targets: Vec<usize>,
    syscalls: Vec<u32>,
    reserves: Vec<u8>,
}

fn operand(code: &[u8], cursor: usize) -> usize {
    let byte = code[cursor];
    let mode = byte >> 6;
    let low = byte & 7;
    if mode == 3 {
        return 1;
    }
    let sib = usize::from(low == 4);
    let disp = match (mode, low) {
        (0, 5) | (2, _) => 4,
        (1, _) => 1,
        _ => 0,
    };
    1 + sib + disp
}

fn step(code: &[u8], at: usize) -> (usize, Option<usize>, Option<u32>) {
    let mut pos = at;
    if (0x40..=0x4F).contains(&code[pos]) {
        pos += 1;
    }
    match code[pos] {
        0xC3 => (pos + 1 - at, None, None),
        0x50..=0x5F => (pos + 1 - at, None, None),
        0x01 | 0x31 | 0x39 | 0x85 | 0x89 | 0x8B => {
            let extra = operand(code, pos + 1);
            (pos + 1 + extra - at, None, None)
        }
        0x8D => {
            let extra = operand(code, pos + 1);
            let rel = (code[pos + 1] & 0xC7 == 0x05).then(|| {
                let raw = i32::from_le_bytes(code[pos + 2..pos + 6].try_into().unwrap());
                ((pos + 6) as isize + raw as isize) as usize
            });
            (pos + 1 + extra - at, rel, None)
        }
        0x83 => {
            let extra = operand(code, pos + 1);
            (pos + 2 + extra - at, None, None)
        }
        0x81 | 0xC7 => {
            let extra = operand(code, pos + 1);
            (pos + 5 + extra - at, None, None)
        }
        0xC1 => {
            let extra = operand(code, pos + 1);
            (pos + 2 + extra - at, None, None)
        }
        0xB8..=0xBF => {
            let imm = u32::from_le_bytes(code[pos + 1..pos + 5].try_into().unwrap());
            let call = (code[pos] == 0xB8 && pos == at).then_some(imm);
            (pos + 5 - at, None, call)
        }
        0xE8 | 0xE9 => {
            let raw = i32::from_le_bytes(code[pos + 1..pos + 5].try_into().unwrap());
            let dest = ((pos + 5) as isize + raw as isize) as usize;
            (pos + 5 - at, Some(dest), None)
        }
        0xFF => {
            let extra = operand(code, pos + 1);
            (pos + 1 + extra - at, None, None)
        }
        0x0F => branch(code, at, pos),
        other => panic!("unexpected opcode 0x{other:02X} at {at}"),
    }
}

fn branch(code: &[u8], at: usize, pos: usize) -> (usize, Option<usize>, Option<u32>) {
    match code[pos + 1] {
        0x05 | 0x0B => (pos + 2 - at, None, None),
        0x80..=0x8F => {
            let raw = i32::from_le_bytes(code[pos + 2..pos + 6].try_into().unwrap());
            let dest = ((pos + 6) as isize + raw as isize) as usize;
            (pos + 6 - at, Some(dest), None)
        }
        other => panic!("unexpected secondary opcode 0x{other:02X} at {at}"),
    }
}

fn inspect(stub: &Stub) -> Analysis {
    let mut boundaries = BTreeSet::new();
    let mut targets = Vec::new();
    let mut syscalls = Vec::new();
    let mut reserves = Vec::new();
    let mut cursor = 0;
    let mut recent = None;
    while cursor < stub.code.len() {
        boundaries.insert(cursor);
        if stub.code[cursor..].starts_with(&[
            0x55, 0x48, 0x89, 0xE5, 0x53, 0x41, 0x54, 0x41, 0x55, 0x41, 0x56, 0x41, 0x57, 0x48,
            0x83, 0xEC,
        ]) {
            reserves.push(stub.code[cursor + 16]);
        }
        if stub.code[cursor..].starts_with(&[0x0F, 0x05])
            && let Some(num) = recent
        {
            syscalls.push(num);
        }
        let (len, target, eax) = step(&stub.code, cursor);
        if let Some(dest) = target {
            targets.push(dest);
        }
        if eax.is_some() {
            recent = eax;
        }
        cursor += len;
    }
    assert_eq!(cursor, stub.code.len());
    Analysis {
        boundaries,
        targets,
        syscalls,
        reserves,
    }
}

#[test]
fn integrity() {
    for item in stubs() {
        let report = inspect(&item);
        for dest in &report.targets {
            assert!(
                report.boundaries.contains(dest),
                "unaligned branch target {dest} in {:?}",
                item.hook
            );
        }
        for amount in &report.reserves {
            assert_eq!(
                (8 + 48 + usize::from(*amount)) % 16,
                0,
                "misaligned stack frame in {:?}",
                item.hook
            );
        }
    }
}

#[test]
fn kernel() {
    for item in stubs() {
        let report = inspect(&item);
        match item.hook {
            Hook::Start => {
                assert!(report.syscalls.contains(&158));
                assert!(report.syscalls.contains(&231));
            }
            Hook::Exit => {
                assert_eq!(report.syscalls, vec![231, 60]);
            }
            Hook::Write => {
                assert_eq!(report.syscalls, vec![1]);
            }
            Hook::Spawn => {
                assert!(report.syscalls.contains(&9));
                assert!(report.syscalls.contains(&10));
                assert!(report.syscalls.contains(&158));
            }
            Hook::Yield => {
                assert!(report.syscalls.contains(&158));
            }
            Hook::Await | Hook::All | Hook::Race => {
                assert!(report.syscalls.contains(&158));
                assert!(report.syscalls.contains(&11));
            }
            Hook::Cancel => {
                assert!(report.syscalls.contains(&11));
            }
        }
    }
}
