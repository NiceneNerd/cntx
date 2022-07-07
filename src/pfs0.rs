use crate::util::{reader_read_val, ReadSeek, Shared};
use std::io::{Error, ErrorKind, Read, Result, Seek, SeekFrom};

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct Header {
    pub magic: u32,
    pub file_count: u32,
    pub string_table_size: u32,
    pub reserved: [u8; 0x4],
}

impl Header {
    pub const MAGIC: u32 = u32::from_le_bytes(*b"PFS0");
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct FileEntry {
    pub offset: u64,
    pub size: usize,
    pub string_table_offset: u32,
    pub reserved: [u8; 0x4],
}

pub struct PFS0FileReader {
    inner: Shared<dyn ReadSeek>,
    read_offset: u64,
    file_size: u64,
}

unsafe impl Send for PFS0FileReader {}
unsafe impl Sync for PFS0FileReader {}

impl Read for PFS0FileReader {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.inner
            .lock()
            .unwrap()
            .seek(SeekFrom::Start(self.read_offset))?;
        self.inner.lock().unwrap().read(buf)
    }
}

impl Seek for PFS0FileReader {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        match pos {
            SeekFrom::Start(offset) => {
                let new_offset = self.read_offset + offset as u64;
                if new_offset > self.file_size {
                    return Err(Error::new(
                        ErrorKind::InvalidInput,
                        "Attempted to seek past end of file",
                    ));
                }
                self.inner
                    .lock()
                    .unwrap()
                    .seek(SeekFrom::Start(self.read_offset + offset))
            }
            SeekFrom::Current(offset) => {
                let new_offset =
                    (self.inner.lock().unwrap().stream_position()? as i64 + offset) as u64;
                if new_offset > self.read_offset + self.file_size || new_offset < self.read_offset {
                    return Err(Error::new(ErrorKind::InvalidInput, "Seek out of bounds"));
                }
                self.inner.lock().unwrap().seek(SeekFrom::Start(new_offset))
            }
            SeekFrom::End(offset) => {
                let new_offset = (self.read_offset as i64 + self.file_size as i64 + offset) as u64;
                if new_offset > self.read_offset + self.file_size || new_offset < self.read_offset {
                    return Err(Error::new(ErrorKind::InvalidInput, "Seek out of bounds"));
                }
                self.inner.lock().unwrap().seek(SeekFrom::End(offset))
            }
        }
    }
}

pub struct PFS0 {
    reader: Shared<dyn ReadSeek>,
    header: Header,
    file_entries: Vec<FileEntry>,
    string_table: Vec<u8>,
}

impl PFS0 {
    pub fn new(reader: Shared<dyn ReadSeek>) -> Result<Self> {
        let header: Header = reader_read_val(&reader)?;
        if header.magic != Header::MAGIC {
            return Err(Error::new(ErrorKind::InvalidInput, "Invalid PFS0 magic"));
        }

        let mut file_entries: Vec<FileEntry> = Vec::with_capacity(header.file_count as usize);

        for _ in 0..header.file_count {
            let file_entry: FileEntry = reader_read_val(&reader)?;
            file_entries.push(file_entry);
        }

        let mut str_table = vec![0u8; header.string_table_size as usize];
        reader.lock().unwrap().read_exact(&mut str_table)?;

        Ok(Self {
            reader,
            header,
            file_entries,
            string_table: str_table,
        })
    }

    pub fn list_files(&self) -> Result<Vec<String>> {
        let mut file_names: Vec<String> = Vec::with_capacity(self.file_entries.len());

        for entry in self.file_entries.iter() {
            let mut bytes: Vec<u8> = Vec::new();

            let str_t = &self.string_table[entry.string_table_offset as usize..];
            for c in str_t {
                if *c == 0 {
                    break;
                }

                bytes.push(*c);
            }

            file_names.push(String::from_utf8(bytes).unwrap());
        }

        Ok(file_names)
    }

    pub fn get_file_size(&mut self, idx: usize) -> Result<usize> {
        if idx >= self.file_entries.len() {
            return Err(Error::new(ErrorKind::InvalidInput, "Invalid file index"));
        }

        Ok(self.file_entries[idx].size)
    }

    pub fn read_file(&mut self, idx: usize, offset: usize, buf: &mut [u8]) -> Result<usize> {
        if idx >= self.file_entries.len() {
            return Err(Error::new(ErrorKind::InvalidInput, "Invalid file index"));
        }

        let entry = &self.file_entries[idx];
        if (offset + buf.len()) > entry.size {
            return Err(Error::new(ErrorKind::UnexpectedEof, "EOF reached"));
        }

        let base_offset = std::mem::size_of::<Header>()
            + std::mem::size_of::<FileEntry>() * self.header.file_count as usize
            + self.header.string_table_size as usize;
        let base_read_offset = base_offset + entry.offset as usize;
        let read_offset = base_read_offset + offset;

        self.reader
            .lock()
            .unwrap()
            .seek(SeekFrom::Start(read_offset as u64))?;
        self.reader.lock().unwrap().read(buf)
    }

    pub fn get_file_reader(&mut self, idx: usize) -> Result<PFS0FileReader> {
        if idx >= self.file_entries.len() {
            return Err(Error::new(ErrorKind::InvalidInput, "Invalid file index"));
        }

        let entry = &self.file_entries[idx];
        let base_offset = std::mem::size_of::<Header>()
            + std::mem::size_of::<FileEntry>() * self.header.file_count as usize
            + self.header.string_table_size as usize;
        let base_read_offset = base_offset + entry.offset as usize;
        let read_offset = base_read_offset + entry.size as usize;
        let reader = self.reader.clone();
        let mut reader = PFS0FileReader {
            inner: reader,
            read_offset: read_offset as u64,
            file_size: entry.size as u64,
        };
        reader.seek(SeekFrom::Start(read_offset as u64))?;
        Ok(reader)
    }
}
