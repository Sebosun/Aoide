use crate::{ParseError, ParseOptions};
use std::io::{Cursor, Read};
use zip::ZipArchive;

pub(crate) struct Archive<'a> {
    zip: ZipArchive<Cursor<&'a [u8]>>,
    max_entry_bytes: u64,
}

impl<'a> Archive<'a> {
    pub fn open(bytes: &'a [u8], options: &ParseOptions) -> Result<Self, ParseError> {
        if bytes.len() as u64 > options.max_archive_bytes {
            return Err(ParseError::LimitExceeded("archive bytes"));
        }
        let mut archive = ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| ParseError::Invalid(error.to_string()))?;
        if archive.len() > options.max_entries {
            return Err(ParseError::LimitExceeded("entry count"));
        }
        let mut total = 0u64;
        for index in 0..archive.len() {
            let file = archive
                .by_index_raw(index)
                .map_err(|error| ParseError::Invalid(error.to_string()))?;
            let path = file.name();
            if file.encrypted() {
                return Err(ParseError::UnsupportedEncryption(path.into()));
            }
            if file.size() > options.max_entry_bytes {
                return Err(ParseError::LimitExceeded("entry bytes"));
            }
            total = total
                .checked_add(file.size())
                .ok_or(ParseError::LimitExceeded("uncompressed bytes"))?;
            if total > options.max_uncompressed_bytes {
                return Err(ParseError::LimitExceeded("uncompressed bytes"));
            }
            if path.starts_with('/')
                || path.contains(['\\', '\0', ':'])
                || path.split('/').any(|part| part == "..")
            {
                return Err(ParseError::UnsafePath(path.into()));
            }
        }
        Ok(Self {
            zip: archive,
            max_entry_bytes: options.max_entry_bytes,
        })
    }

    pub fn read_bytes(&mut self, path: &str) -> Result<Vec<u8>, ParseError> {
        let file = self
            .zip
            .by_name(path)
            .map_err(|error| ParseError::Invalid(format!("{path}: {error}")))?;
        let mut bytes = Vec::new();
        file.take(self.max_entry_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| ParseError::Invalid(format!("{path}: {error}")))?;
        if bytes.len() as u64 > self.max_entry_bytes {
            return Err(ParseError::LimitExceeded("entry bytes"));
        }
        Ok(bytes)
    }

    pub fn read(&mut self, path: &str) -> Result<String, ParseError> {
        let file = self
            .zip
            .by_name(path)
            .map_err(|error| ParseError::Invalid(format!("{path}: {error}")))?;
        let mut text = String::new();
        file.take(self.max_entry_bytes.saturating_add(1))
            .read_to_string(&mut text)
            .map_err(|error| ParseError::Invalid(format!("{path}: {error}")))?;
        if text.len() as u64 > self.max_entry_bytes {
            return Err(ParseError::LimitExceeded("entry bytes"));
        }
        Ok(text)
    }

    pub fn contains(&self, path: &str) -> bool {
        self.zip.file_names().any(|name| name == path)
    }
}
