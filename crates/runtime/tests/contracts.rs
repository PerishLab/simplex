use simplex_runtime::{
    Context, GUARD, Hook, STACK, Syscall, all, cancel, exit, pause, race, spawn, start, stub,
    stubs, wait, write,
};
use std::mem::{align_of, size_of};

fn window(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|part| part == needle)
}

#[test]
fn dimensions() {
    assert_eq!(STACK, 65_536);
    assert_eq!(GUARD, 4_096);
    assert_eq!(STACK + GUARD, 69_632);
    assert_eq!(size_of::<Context>(), 64);
    assert_eq!(align_of::<Context>(), 8);
    assert_eq!(
        Context::default(),
        Context {
            rbx: 0,
            rbp: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rsp: 0,
            rip: 0,
        }
    );
}

#[test]
fn numbers() {
    assert_eq!(Syscall::Read as u32, 0);
    assert_eq!(Syscall::Write as u32, 1);
    assert_eq!(Syscall::Mmap as u32, 9);
    assert_eq!(Syscall::Mprotect as u32, 10);
    assert_eq!(Syscall::Munmap as u32, 11);
    assert_eq!(Syscall::Exit as u32, 60);
}

#[test]
fn symbols() {
    let expected = [
        (Hook::Start, "sx$rt$start"),
        (Hook::Exit, "sx$rt$exit"),
        (Hook::Write, "sx$rt$write"),
        (Hook::Spawn, "sx$rt$spawn"),
        (Hook::Yield, "sx$rt$yield"),
        (Hook::Await, "sx$rt$await"),
        (Hook::All, "sx$rt$all"),
        (Hook::Race, "sx$rt$race"),
        (Hook::Cancel, "sx$rt$cancel"),
    ];
    assert_eq!(Hook::ALL.len(), expected.len());
    for (index, (hook, name)) in expected.into_iter().enumerate() {
        assert_eq!(Hook::ALL[index], hook);
        assert_eq!(hook.symbol(), name);
    }
}

#[test]
fn catalog() {
    let list = stubs();
    assert_eq!(list.len(), 9);
    let direct = [
        start(),
        exit(),
        write(),
        spawn(),
        pause(),
        wait(),
        all(),
        race(),
        cancel(),
    ];
    for (index, item) in list.iter().enumerate() {
        assert_eq!(item, &direct[index]);
        assert_eq!(item, &stub(item.hook));
        assert!(!item.code.is_empty());
    }
}

#[test]
fn primitives() {
    assert_eq!(
        exit().code,
        vec![
            0xB8, 0xE7, 0x00, 0x00, 0x00, 0x0F, 0x05, 0xB8, 0x3C, 0x00, 0x00, 0x00, 0x0F, 0x05,
            0x0F, 0x0B,
        ]
    );
    assert_eq!(
        write().code,
        vec![0xB8, 0x01, 0x00, 0x00, 0x00, 0x0F, 0x05, 0xC3,]
    );
}

#[test]
fn allocation() {
    let bytes = spawn().code;
    let total = (STACK + GUARD) as u32;
    let guard = GUARD as u32;
    let mut mmap = vec![0xBE];
    mmap.extend_from_slice(&total.to_le_bytes());
    let mut protect = vec![0xBE];
    protect.extend_from_slice(&guard.to_le_bytes());
    assert!(window(&bytes, &mmap));
    assert!(window(&bytes, &protect));
    assert!(window(&bytes, &[0x41, 0xBA, 0x22, 0x00, 0x00, 0x00]));
    assert!(window(&bytes, &[0x49, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]));
    assert!(window(&bytes, &[0x4C, 0x8D, 0xA3, 0x00, 0x0F, 0x01, 0x00]));
    assert!(window(&bytes, &[0x48, 0x8D, 0x83, 0xF0, 0x0E, 0x01, 0x00]));
}
