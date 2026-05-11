use crate::ByteRange;

type TextRange = ByteRange;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StringLit {
    pub(crate) range: TextRange,
    pub(crate) content_range: TextRange,
    pub(crate) quote_len: u8,
    pub(crate) prefix: StringPrefix,
    pub(crate) is_terminated: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StringPrefix {
    pub(crate) raw: bool,
    pub(crate) bytes: bool,
    pub(crate) f_string: bool,
    pub(crate) unicode: bool,
}

pub(crate) fn scan_string(src: &[u8], start: usize) -> Option<StringLit> {
    let (quote_start, prefix) = scan_prefix(src, start)?;
    let quote = *src.get(quote_start)?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }

    let quote_len: u8 = if src.get(quote_start..quote_start + 3) == Some(&[quote, quote, quote]) {
        3
    } else {
        1
    };
    let content_start = quote_start + usize::from(quote_len);
    let mut pos = content_start;

    if quote_len == 3 {
        while pos < src.len() {
            if src.get(pos..pos + 3) == Some(&[quote, quote, quote]) && !is_escaped(src, pos) {
                let end = pos + 3;
                return Some(StringLit {
                    range: TextRange::new(start, end),
                    content_range: TextRange::new(content_start, pos),
                    quote_len,
                    prefix,
                    is_terminated: true,
                });
            }
            if !prefix.raw && src[pos] == b'\\' {
                pos = (pos + 2).min(src.len());
            } else {
                pos += 1;
            }
        }
        return Some(StringLit {
            range: TextRange::new(start, src.len()),
            content_range: TextRange::new(content_start, src.len()),
            quote_len,
            prefix,
            is_terminated: false,
        });
    }

    while pos < src.len() {
        match src[pos] {
            b'\n' | b'\r' => {
                return Some(StringLit {
                    range: TextRange::new(start, pos),
                    content_range: TextRange::new(content_start, pos),
                    quote_len,
                    prefix,
                    is_terminated: false,
                });
            }
            b if b == quote => {
                if is_escaped(src, pos) {
                    pos += 1;
                    continue;
                }
                let end = pos + 1;
                return Some(StringLit {
                    range: TextRange::new(start, end),
                    content_range: TextRange::new(content_start, pos),
                    quote_len,
                    prefix,
                    is_terminated: true,
                });
            }
            b'\\' if !prefix.raw => {
                pos = (pos + 2).min(src.len());
            }
            _ => pos += 1,
        }
    }

    Some(StringLit {
        range: TextRange::new(start, src.len()),
        content_range: TextRange::new(content_start, src.len()),
        quote_len,
        prefix,
        is_terminated: false,
    })
}

fn scan_prefix(src: &[u8], start: usize) -> Option<(usize, StringPrefix)> {
    let mut pos = start;
    let mut prefix = StringPrefix::default();
    while let Some(byte) = src.get(pos).copied() {
        match byte {
            b'r' | b'R' => prefix.raw = true,
            b'b' | b'B' => prefix.bytes = true,
            b'f' | b'F' => prefix.f_string = true,
            b'u' | b'U' => prefix.unicode = true,
            b'\'' | b'"' => return Some((pos, prefix)),
            _ => return None,
        }
        pos += 1;
        if pos - start > 3 {
            return None;
        }
    }
    None
}

fn is_escaped(src: &[u8], quote_pos: usize) -> bool {
    let mut backslashes = 0usize;
    let mut pos = quote_pos;
    while pos > 0 && src[pos - 1] == b'\\' {
        backslashes += 1;
        pos -= 1;
    }
    backslashes % 2 == 1
}
