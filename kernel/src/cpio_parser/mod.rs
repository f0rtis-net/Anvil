#[derive(Debug)]
pub struct CpioEntry<'a> {
    pub name: &'a str,
    pub data: &'a [u8],
}

#[derive(Debug)]
pub enum CpioError {
    InvalidMagic,
    InvalidHeader,
    UnexpectedEnd,
    InvalidUtf8,
}

fn parse_hex8(bytes: &[u8]) -> Result<u32, CpioError> {
    if bytes.len() < 8 {
        return Err(CpioError::InvalidHeader);
    }
    let mut result: u32 = 0;
    for &b in &bytes[..8] {
        let digit = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => return Err(CpioError::InvalidHeader),
        };
        result = result * 16 + digit as u32;
    }
    Ok(result)
}

fn align4(x: usize) -> usize {
    (x + 3) & !3
}

pub struct CpioIter<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> CpioIter<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }
}

impl<'a> Iterator for CpioIter<'a> {
    type Item = Result<CpioEntry<'a>, CpioError>;

    fn next(&mut self) -> Option<Self::Item> {
        let data = self.data;
        let off  = self.offset;

        if off + 110 > data.len() {
            return None;
        }

        if &data[off..off + 6] != b"070701" {
            return Some(Err(CpioError::InvalidMagic));
        }

        let filesize = match parse_hex8(&data[off + 54..]) {
            Ok(v) => v as usize,
            Err(e) => return Some(Err(e)),
        };

        let namesize = match parse_hex8(&data[off + 94..]) {
            Ok(v) => v as usize,
            Err(e) => return Some(Err(e)),
        };

        let name_start = off + 110;
        let name_end   = name_start + namesize;

        if name_end > data.len() {
            return Some(Err(CpioError::UnexpectedEnd));
        }

        let name_bytes = &data[name_start..name_end - 1];
        let name = match core::str::from_utf8(name_bytes) {
            Ok(s) => s,
            Err(_) => return Some(Err(CpioError::InvalidUtf8)),
        };

        let data_start = align4(name_end);
        let data_end   = data_start + filesize;

        if data_end > data.len() {
            return Some(Err(CpioError::UnexpectedEnd));
        }

        let file_data = &data[data_start..data_end];

        self.offset = align4(data_end);

        if name == "TRAILER!!!" {
            return None;
        }

        Some(Ok(CpioEntry {
            name,
            data: file_data,
        }))
    }
}

pub fn cpio_find<'a>(archive: &'a [u8], filename: &str) -> Option<&'a [u8]> {
    for entry in CpioIter::new(archive) {
        if let Ok(e) = entry {
            if e.name == filename || e.name.ends_with(filename) {
                return Some(e.data);
            }
        }
    }
    None
}