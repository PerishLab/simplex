use simplex_elf::Kind;
use simplex_emit::Register;
use simplex_emit::encode::media::{Arithmetic, Precision};
use simplex_emit::encode::{Address, Condition, Encoder, Shift, Width};

#[test]
fn constants() {
    let mut enc = Encoder::new();
    enc.constant(Register::Rax, 0);
    enc.constant(Register::Rcx, 42);
    enc.constant(Register::R8, -7);
    enc.constant(Register::R12, 0x1_0000_0005);
    let (bytes, relocs) = enc.finish().expect("encode constants");
    assert!(relocs.is_empty());
    assert_eq!(&bytes[0..2], &[0x31, 0xC0]);
    assert_eq!(&bytes[2..7], &[0xB9, 42, 0, 0, 0]);
    assert_eq!(&bytes[7..14], &[0x49, 0xC7, 0xC0, 0xF9, 0xFF, 0xFF, 0xFF]);
    assert_eq!(bytes.len(), 24);
}

#[test]
fn addressing() {
    let mut enc = Encoder::new();
    enc.load(Register::Rax, Address::new(Register::Rsp, 0), Width::Quad);
    enc.load(Register::Rcx, Address::new(Register::R12, 0), Width::Quad);
    enc.load(Register::Rdx, Address::new(Register::Rbp, 0), Width::Quad);
    enc.load(Register::Rsi, Address::new(Register::R13, -8), Width::Quad);
    enc.store(Address::new(Register::Rbp, -16), Register::Rdi, Width::Byte);
    enc.signed(Register::R9, Address::new(Register::Rbp, -4), Width::Double);
    let (bytes, _) = enc.finish().expect("encode memory");
    assert_eq!(&bytes[0..4], &[0x48, 0x8B, 0x04, 0x24]);
    assert_eq!(&bytes[4..8], &[0x49, 0x8B, 0x0C, 0x24]);
    assert_eq!(&bytes[8..12], &[0x48, 0x8B, 0x55, 0x00]);
    assert_eq!(&bytes[12..16], &[0x49, 0x8B, 0x75, 0xF8]);
    assert_eq!(&bytes[16..20], &[0x40, 0x88, 0x7D, 0xF0]);
    assert_eq!(&bytes[20..24], &[0x4C, 0x63, 0x4D, 0xFC]);
}

#[test]
fn integer() {
    let mut enc = Encoder::new();
    enc.add(Register::Rax, Register::Rcx);
    enc.sub(Register::Rax, Register::Rdx);
    enc.multiply(Register::Rax, Register::R8);
    enc.divide(true);
    enc.divide(false);
    enc.bitand(Register::Rax, Register::Rsi);
    enc.bitor(Register::Rax, Register::Rdi);
    enc.bitxor(Register::Rax, Register::R9);
    enc.shift(Shift::Left, Register::Rax);
    enc.shift(Shift::Logical, Register::Rax);
    enc.shift(Shift::Arithmetic, Register::Rax);
    enc.extend(Register::Rax, Register::Rsi, Width::Byte);
    enc.widen(Register::Rax, Register::Rdi, Width::Word);
    enc.transfer();
    let (bytes, _) = enc.finish().expect("encode integer");
    assert_eq!(&bytes[0..3], &[0x48, 0x01, 0xC8]);
    assert_eq!(&bytes[3..6], &[0x48, 0x29, 0xD0]);
    assert_eq!(&bytes[6..10], &[0x49, 0x0F, 0xAF, 0xC0]);
    assert_eq!(&bytes[bytes.len() - 3..], &[0xFC, 0xF3, 0xA4]);
}

#[test]
fn branches() {
    let mut enc = Encoder::new();
    let skip = enc.fresh();
    enc.compare(Register::Rdi, Register::Rsi);
    enc.branch(Condition::Equal, skip);
    enc.constant(Register::Rax, 1);
    enc.bind(skip);
    enc.flag(Register::Rsi, Condition::Less);
    enc.call("sx$app$main$helper", false);
    enc.call("write", true);
    enc.symbol(Register::Rax, "sx$ro$app$main$0");
    enc.global(Register::Rcx, "sx$vt$ext");
    enc.indirect(Register::R11);
    enc.leave();
    let (bytes, relocs) = enc.finish().expect("encode branches");
    assert_eq!(&bytes[0..3], &[0x48, 0x39, 0xF7]);
    assert_eq!(&bytes[3..9], &[0x0F, 0x84, 5, 0, 0, 0]);
    assert_eq!(bytes[bytes.len() - 1], 0xC3);
    assert_eq!(relocs.len(), 4);
    assert_eq!(relocs[0].kind, Kind::Relative32);
    assert_eq!(relocs[1].kind, Kind::Procedure32);
    assert_eq!(relocs[2].kind, Kind::Relative32);
    assert_eq!(relocs[3].kind, Kind::Global32);
    assert!(relocs.iter().all(|r| r.addend == -4));
}

#[test]
fn media() {
    let mut enc = Encoder::new();
    enc.ingest(0, Register::Rax, Precision::Double);
    enc.ingest(1, Register::Rcx, Precision::Double);
    enc.scalar(Arithmetic::Add, Precision::Double);
    enc.scalar(Arithmetic::Sub, Precision::Double);
    enc.scalar(Arithmetic::Mul, Precision::Double);
    enc.scalar(Arithmetic::Div, Precision::Double);
    enc.extract(Register::Rax, 0, Precision::Double);
    enc.relation(Register::Rax, "==", Precision::Double);
    enc.promote(Register::Rax, Register::Rdi, Precision::Double);
    enc.unsigned(Register::Rax, Register::Rsi, Precision::Double);
    enc.truncate(Register::Rax, Register::Rdx, Precision::Double);
    enc.resize(Register::Rax, Register::Rcx, Precision::Single);
    let (bytes, _) = enc.finish().expect("encode sse2");
    assert_eq!(&bytes[0..5], &[0x66, 0x48, 0x0F, 0x6E, 0xC0]);
    assert!(!bytes.is_empty());
}
