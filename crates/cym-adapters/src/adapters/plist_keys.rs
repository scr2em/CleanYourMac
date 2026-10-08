//! Reads a few top-level keys of a property list without trusting the file: no symbolic
//! links, no FIFOs or devices, at most 1 MB, and a bounded number of parse events, so a
//! crafted `Info.plist` (for example one whose binary objects refer to each other many
//! times over) can neither hang nor exhaust memory.
use std::{collections::HashMap, io::Read};

const MAX_BYTES: u64 = 1 << 20;

/// A scalar value read from a property list.
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    Text(String),
    /// Seconds since 1970.
    Date(f64),
    Number(f64),
    Bool(bool),
}
impl Scalar {
    pub fn text(&self) -> Option<&str> {
        match self {
            Scalar::Text(t) => Some(t),
            _ => None,
        }
    }
}

/// The values of `keys` in the property list's top-level dictionary.
pub fn read(path: &str, keys: &[&str]) -> Option<HashMap<String, Scalar>> {
    let mut file = open(path)?;
    let mut bytes = vec![];
    (&mut file)
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    parse(&bytes, keys)
}

#[cfg(unix)]
fn open(path: &str) -> Option<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    file.metadata().ok()?.is_file().then_some(file)
}
#[cfg(not(unix))]
fn open(path: &str) -> Option<std::fs::File> {
    let file = std::fs::File::open(path).ok()?;
    file.metadata().ok()?.is_file().then_some(file)
}

/// The values of `keys` in a property list's top-level dictionary, from its bytes.
pub fn parse(bytes: &[u8], keys: &[&str]) -> Option<HashMap<String, Scalar>> {
    if bytes.starts_with(b"bplist00") {
        return binary(bytes, keys);
    }
    // An XML property list has no references between objects, so parsing it whole is
    // bounded by its size; its nesting is bounded first, because the parser recurses and a
    // stack overflow cannot be caught.
    if xml_depth(bytes) > MAX_DEPTH {
        return None;
    }
    let value = plist::Value::from_reader_xml(std::io::Cursor::new(bytes)).ok()?;
    let dict = value.as_dictionary()?;
    let mut found = HashMap::new();
    for key in keys {
        let value = match dict.get(key) {
            Some(plist::Value::String(t)) => Scalar::Text(t.clone()),
            Some(plist::Value::Date(d)) => Scalar::Date(
                std::time::SystemTime::from(*d)
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0.0, |d| d.as_secs_f64()),
            ),
            Some(plist::Value::Integer(i)) => Scalar::Number(i.as_signed().unwrap_or(0) as f64),
            Some(plist::Value::Real(r)) => Scalar::Number(*r),
            Some(plist::Value::Boolean(b)) => Scalar::Bool(*b),
            _ => continue,
        };
        found.insert((*key).to_owned(), value);
    }
    Some(found)
}

const MAX_DEPTH: usize = 128;
/// The deepest element nesting, counted without parsing. Every element counts, whatever its
/// name or prefix; comments, processing instructions, CDATA and quoted attribute values are
/// skipped. Anything this scan cannot follow (an internal DTD subset, which could declare
/// markup, or a mismatched or unterminated tag) counts as too deep, so the recursive parser
/// only ever sees documents whose nesting was measured.
fn xml_depth(bytes: &[u8]) -> usize {
    const REFUSE: usize = usize::MAX;
    let find = |from: usize, pattern: &[u8]| -> Option<usize> {
        bytes
            .get(from..)?
            .windows(pattern.len())
            .position(|w| w == pattern)
            .map(|p| p + from)
    };
    // The position of the `>` closing a tag that starts at `from`, outside quoted values.
    let tag_end = |from: usize| -> Option<usize> {
        let mut quote = None;
        for (i, b) in bytes.get(from..)?.iter().enumerate() {
            match (quote, *b) {
                (None, b'"' | b'\'') => quote = Some(*b),
                (Some(q), b) if b == q => quote = None,
                (None, b'>') => return Some(from + i),
                _ => {}
            }
        }
        None
    };
    let mut open: Vec<&[u8]> = vec![];
    let mut deepest = 0;
    let mut at = 0;
    while let Some(next) = find(at, b"<") {
        let rest = &bytes[next..];
        let skip = if rest.starts_with(b"<!--") {
            find(next + 4, b"-->").map(|e| e + 3)
        } else if rest.starts_with(b"<![CDATA[") {
            find(next + 9, b"]]>").map(|e| e + 3)
        } else if rest.starts_with(b"<?") {
            find(next + 2, b"?>").map(|e| e + 2)
        } else if rest.starts_with(b"<!-") {
            None
        } else if rest.starts_with(b"<!") {
            match tag_end(next + 2) {
                Some(e) if !bytes[next..e].contains(&b'[') => Some(e + 1),
                _ => None,
            }
        } else {
            let Some(e) = tag_end(next + 1) else {
                return REFUSE;
            };
            let tag = &bytes[next + 1..e];
            let (closing, body) = match tag.strip_prefix(b"/") {
                Some(body) => (true, body),
                None => (false, tag),
            };
            let name = &body[..body
                .iter()
                .position(|b| b.is_ascii_whitespace() || *b == b'/')
                .unwrap_or(body.len())];
            if name.is_empty() {
                return REFUSE;
            }
            if closing {
                if open.pop() != Some(name) {
                    return REFUSE;
                }
            } else if tag.ends_with(b"/") {
                // The parser expands an empty element into a start and an end.
                deepest = deepest.max(open.len() + 1);
            } else {
                open.push(name);
                deepest = deepest.max(open.len());
                if deepest > MAX_DEPTH {
                    return deepest;
                }
            }
            Some(e + 1)
        };
        let Some(skip) = skip else {
            return REFUSE;
        };
        at = skip;
    }
    deepest
}

/// Seconds between 1970 and 2001, the epoch of binary property-list dates.
const APPLE_EPOCH: f64 = 978_307_200.0;

/// A binary property list: only the root dictionary's keys and scalar values are read,
/// each object at most once, so shared references cannot multiply the work.
fn binary(bytes: &[u8], keys: &[&str]) -> Option<HashMap<String, Scalar>> {
    let trailer = bytes.get(bytes.len().checked_sub(32)?..)?;
    let offset_size = trailer[6] as usize;
    let ref_size = trailer[7] as usize;
    let be = |data: &[u8]| -> Option<u64> {
        (data.len() <= 8).then(|| data.iter().fold(0u64, |n, b| (n << 8) | u64::from(*b)))
    };
    let objects = be(&trailer[8..16])? as usize;
    let top = be(&trailer[16..24])? as usize;
    let table = be(&trailer[24..32])? as usize;
    if !(1..=8).contains(&offset_size) || !(1..=8).contains(&ref_size) || top >= objects {
        return None;
    }
    let offset = |index: usize| -> Option<usize> {
        let at = table.checked_add(index.checked_mul(offset_size)?)?;
        Some(be(bytes.get(at..at.checked_add(offset_size)?)?)? as usize)
    };
    // An object's marker, and its length with the position of its first content byte.
    let header = |at: usize| -> Option<(u8, usize, usize)> {
        let marker = *bytes.get(at)?;
        let low = (marker & 0x0F) as usize;
        if low != 0x0F || marker >> 4 == 0 || marker >> 4 == 0x3 {
            return Some((marker, low, at + 1));
        }
        let int = *bytes.get(at + 1)?;
        if int >> 4 != 0x1 {
            return None;
        }
        let width = 1usize << (int & 0x0F);
        let length = be(bytes.get(at + 2..at + 2 + width)?)? as usize;
        Some((marker, length, at + 2 + width))
    };
    // Objects may be shared, so decoding is budgeted in total, not per object: a crafted
    // file cannot make many keys decode one large string over and over.
    let budget = std::cell::Cell::new(MAX_BYTES as usize * 2);
    let string = |index: usize, limit: usize| -> Option<String> {
        let (marker, length, start) = header(offset(index)?)?;
        if length > limit || length > budget.get() {
            return None;
        }
        budget.set(budget.get() - length);
        match marker >> 4 {
            0x5 => String::from_utf8(bytes.get(start..start.checked_add(length)?)?.to_vec()).ok(),
            0x6 => {
                let raw = bytes.get(start..start.checked_add(length.checked_mul(2)?)?)?;
                let units: Vec<u16> = raw
                    .chunks(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect();
                String::from_utf16(&units).ok()
            }
            _ => None,
        }
    };
    let scalar = |index: usize| -> Option<Scalar> {
        let at = offset(index)?;
        let marker = *bytes.get(at)?;
        match marker >> 4 {
            0x5 | 0x6 => string(index, 64 * 1024).map(Scalar::Text),
            0x3 => {
                let raw: [u8; 8] = bytes.get(at + 1..at + 9)?.try_into().ok()?;
                Some(Scalar::Date(f64::from_be_bytes(raw) + APPLE_EPOCH))
            }
            0x0 if marker == 0x08 || marker == 0x09 => Some(Scalar::Bool(marker == 0x09)),
            0x1 => {
                let width = 1usize << (marker & 0x0F);
                Some(Scalar::Number(
                    be(bytes.get(at + 1..at + 1 + width)?)? as f64
                ))
            }
            _ => None,
        }
    };
    let (marker, count, start) = header(offset(top)?)?;
    if marker >> 4 != 0xD || count > 100_000 {
        return None;
    }
    let reference = |position: usize| -> Option<usize> {
        let at = start.checked_add(position.checked_mul(ref_size)?)?;
        Some(be(bytes.get(at..at.checked_add(ref_size)?)?)? as usize)
    };
    let mut found = HashMap::new();
    for i in 0..count {
        // Wanted keys are short names; longer keys are skipped without decoding.
        let Some(key) = string(reference(i)?, 128) else {
            continue;
        };
        if keys.contains(&key.as_str()) && !found.contains_key(&key) {
            if let Some(value) = scalar(reference(count + i)?) {
                found.insert(key, value);
            }
        }
        if found.len() == keys.len() {
            break;
        }
    }
    Some(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_top_level_scalars() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>
            <key>Nested</key><dict><key>CFBundleIdentifier</key><string>wrong</string></dict>
            <key>CFBundleIdentifier</key><string>com.apple.dt.Xcode</string>
            <key>List</key><array><string>a</string></array>
            <key>CFBundleShortVersionString</key><string>16.2</string>
        </dict></plist>"#;
        let values = parse(xml, &["CFBundleIdentifier", "CFBundleShortVersionString"]).unwrap();
        assert_eq!(
            values["CFBundleIdentifier"].text(),
            Some("com.apple.dt.Xcode")
        );
        assert_eq!(values["CFBundleShortVersionString"].text(), Some("16.2"));
        assert_eq!(parse(b"not a plist", &["x"]), None);
        // The same values, written as a binary property list.
        let mut binary = vec![];
        plist::Value::from_reader_xml(std::io::Cursor::new(&xml[..]))
            .unwrap()
            .to_writer_binary(&mut binary)
            .unwrap();
        let values = parse(
            &binary,
            &["CFBundleIdentifier", "CFBundleShortVersionString"],
        )
        .unwrap();
        assert_eq!(
            values["CFBundleIdentifier"].text(),
            Some("com.apple.dt.Xcode")
        );
        assert_eq!(values["CFBundleShortVersionString"].text(), Some("16.2"));
        // Many keys sharing one large string: decoding stays within its budget.
        let started = std::time::Instant::now();
        let _ = parse(
            &shared_string_bomb(60_000, 900_000),
            &["CFBundleIdentifier"],
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        // Deep nesting is refused before the recursive XML parser sees it.
        let deep = format!(
            "<plist><dict><key>K</key>{}{}</dict></plist>",
            "<array>".repeat(100_000),
            "</array>".repeat(100_000)
        );
        assert_eq!(parse(deep.as_bytes(), &["K"]), None);
        assert_eq!(
            xml_depth(b"<dict><array/><array><dict></dict></array></dict>"),
            3
        );
        assert_eq!(
            xml_depth(b"<dict><array><dict><array/></dict></array></dict>"),
            4
        );
        // Closing tags the parser never sees, and names the count would not recognise, do
        // not hide nesting.
        let deep_in = |open: &str, close: &str| {
            format!(
                "<plist><dict><key>K</key>{}{}</dict></plist>",
                open.repeat(100_000),
                close.repeat(100_000)
            )
        };
        for (open, close) in [
            ("<array><!--</array>-->", "</array>"),
            ("<array><![CDATA[</array>]]>", "</array>"),
            ("<array><?x </array>?>", "</array>"),
            ("<x:array>", "</x:array>"),
            ("<ARRAY>", "</ARRAY>"),
            ("<array a=\"x/>\">", "</array>"),
            ("<array a='</array>'>", "</array>"),
        ] {
            assert_eq!(
                parse(deep_in(open, close).as_bytes(), &["K"]),
                None,
                "{open}"
            );
        }
        // Mismatched tags and internal DTD subsets are refused outright.
        assert_eq!(xml_depth(b"<a></b>"), usize::MAX);
        assert_eq!(
            xml_depth(b"<!DOCTYPE p [<!ENTITY e \"<a>\">]><p/>"),
            usize::MAX
        );
        // A real Info.plist header still parses.
        let header = br#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<!-- built by Xcode -->
<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>a.b</string></dict></plist>"#;
        assert_eq!(
            parse(header, &["CFBundleIdentifier"]).unwrap()["CFBundleIdentifier"].text(),
            Some("a.b")
        );
        // Truncated or nonsense binary data is refused, not trusted.
        assert_eq!(parse(&binary[..binary.len() - 5], &["x"]), None);
    }

    /// A binary property list whose root dictionary has `keys` entries that all refer to one
    /// string object of `length` bytes.
    fn shared_string_bomb(keys: usize, length: usize) -> Vec<u8> {
        let mut out = b"bplist00".to_vec();
        let mut offsets = vec![out.len()];
        // Object 0: the dictionary, with a 4-byte count and 1-byte references to object 1.
        out.push(0xDF);
        out.push(0x12);
        out.extend((keys as u32).to_be_bytes());
        out.extend(std::iter::repeat_n(1u8, keys * 2));
        offsets.push(out.len());
        // Object 1: an ASCII string with a 4-byte length.
        out.push(0x5F);
        out.push(0x12);
        out.extend((length as u32).to_be_bytes());
        out.extend(std::iter::repeat_n(b'a', length));
        let table = out.len();
        for offset in &offsets {
            out.extend((*offset as u32).to_be_bytes());
        }
        out.extend([0u8; 6]);
        out.push(4); // offset size
        out.push(1); // reference size
        out.extend((offsets.len() as u64).to_be_bytes());
        out.extend(0u64.to_be_bytes());
        out.extend((table as u64).to_be_bytes());
        out
    }
}
