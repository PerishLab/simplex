pub fn digit(byte: u8, radix: u32) -> bool {
    (byte as char).is_digit(radix)
}

pub fn span(first: u8, second: u8, third: u8, spaced: bool) -> usize {
    if spaced
        && matches!(
            (first, second, third),
            (b'<', b'<', b'=') | (b'>', b'>', b'=')
        )
    {
        return 3;
    }
    if spaced && matches!((first, second), (b'<', b'<') | (b'>', b'>')) {
        return 2;
    }
    if pair(first, second) {
        return 2;
    }
    usize::from(punctuation(first))
}

pub fn single(text: &str) -> bool {
    if text == "_" {
        return true;
    }
    if text.contains('_') {
        return false;
    }
    let mut lowered = false;
    for (index, ch) in text.chars().enumerate() {
        if ch.is_ascii_lowercase() {
            lowered = true;
        } else if ch.is_ascii_uppercase() && (index > 0 || lowered) {
            return false;
        }
    }
    true
}

fn pair(first: u8, second: u8) -> bool {
    matches!(
        (first, second),
        (b':', b'=')
            | (b'=', b'>')
            | (b'-', b'>')
            | (b'=', b'=')
            | (b'!', b'=')
            | (b'<', b'=')
            | (b'>', b'=')
            | (b'+', b'=')
            | (b'-', b'=')
            | (b'*', b'=')
            | (b'/', b'=')
            | (b'%', b'=')
            | (b'&', b'=')
            | (b'|', b'=')
            | (b'^', b'=')
            | (b'&', b'&')
            | (b'|', b'|')
            | (b'.', b'.')
            | (b':', b':')
    )
}

fn punctuation(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')'
            | b'['
            | b']'
            | b'{'
            | b'}'
            | b','
            | b'.'
            | b':'
            | b'='
            | b'+'
            | b'-'
            | b'*'
            | b'/'
            | b'%'
            | b'&'
            | b'|'
            | b'^'
            | b'!'
            | b'<'
            | b'>'
            | b'?'
            | b'@'
    )
}
