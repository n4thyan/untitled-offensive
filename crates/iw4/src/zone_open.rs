//! MW2/IW4 fastfile zone opening and walking.
//!
//! Adapted from mabaro/iw4L_modernWarfare2 (Apache-2.0).
//! Only the IW4 path is implemented; T5/IW5 are not needed.

use std::borrow::Cow;
use std::path::Path;

use flate2::Decompress;

use crate::{
    FileHeaderError, MAGIC_AUTH_HEADER, Signing, XFILE_HEADER_LEN, ZoneError, ZoneHeader,
    ZoneStream, parse_file_header, parse_zone_header, read_wire_asset_table, Iw4WireFormat,
    WireAssetTable, WireTableError,
};

const AUTHED_CHUNK_SIZE: usize = 0x2000;
const AUTHED_CHUNKS_PER_GROUP: usize = 256;

#[derive(Debug)]
pub enum ZoneOpenError {
    Io(std::io::Error),
    Iw4Header(FileHeaderError),
    BadAuthHeader,
    Inflate(String),
    Truncated,
    Iw4WireLayout {
        x86: WireTableError,
        x64: WireTableError,
    },
    AmbiguousIw4WireLayout,
}

impl std::fmt::Display for ZoneOpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "reading zone: {e}"),
            Self::Iw4Header(e) => write!(f, "bad IW4 fastfile header: {e:?}"),
            Self::BadAuthHeader => write!(f, "signed zone without a valid auth header"),
            Self::Inflate(e) => write!(f, "inflate failed: {e}"),
            Self::Truncated => write!(f, "inflated zone is shorter than its XFile header"),
            Self::Iw4WireLayout { x86, x64 } => write!(
                f,
                "invalid IW4 asset table: x86 candidate {x86:?}; x64 candidate {x64:?}"
            ),
            Self::AmbiguousIw4WireLayout => {
                write!(f, "ambiguous IW4 asset table: both x86 and x64 layouts validate")
            }
        }
    }
}

impl std::error::Error for ZoneOpenError {}

pub struct ZoneImage {
    pub bytes: Vec<u8>,
    pub iw4_wire_format: Option<Iw4WireFormat>,
}

impl ZoneImage {
    pub fn header(&self) -> Result<ZoneHeader, ZoneOpenError> {
        parse_zone_header(&self.bytes).map_err(|_| ZoneOpenError::Truncated)
    }
}

pub fn open_zone(path: impl AsRef<Path>) -> Result<ZoneImage, ZoneOpenError> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(ZoneOpenError::Io)?;
    parse_zone_image(&bytes)
}

fn select_iw4_table(image: &[u8]) -> Result<WireAssetTable<'_>, ZoneOpenError> {
    match (
        read_wire_asset_table(image, Iw4WireFormat::X86),
        read_wire_asset_table(image, Iw4WireFormat::X64),
    ) {
        (Ok(table), Err(_)) | (Err(_), Ok(table)) => Ok(table),
        (Ok(_), Ok(_)) => Err(ZoneOpenError::AmbiguousIw4WireLayout),
        (Err(x86), Err(x64)) => Err(ZoneOpenError::Iw4WireLayout { x86, x64 }),
    }
}

fn inflate_zone(bytes: &[u8]) -> Result<Vec<u8>, ZoneOpenError> {
    let mut decoder = Decompress::new(true);
    let mut out = Vec::with_capacity(bytes.len());
    loop {
        out.reserve((out.len() / 2).max(1024 * 1024));
        let before = (decoder.total_in(), decoder.total_out());
        let status = decoder
            .decompress_vec(
                &bytes[decoder.total_in() as usize..],
                &mut out,
                flate2::FlushDecompress::None,
            )
            .map_err(|e| ZoneOpenError::Inflate(e.to_string()))?;
        if status == flate2::Status::StreamEnd {
            return Ok(out);
        }
        if before == (decoder.total_in(), decoder.total_out()) {
            return Err(ZoneOpenError::Inflate("truncated zlib stream".into()));
        }
    }
}

fn parse_zone_image(bytes: &[u8]) -> Result<ZoneImage, ZoneOpenError> {
    let header = parse_file_header(bytes).map_err(ZoneOpenError::Iw4Header)?;
    let body = &bytes[crate::FILE_PREAMBLE_LEN..];

    let compressed: Cow<'_, [u8]> = match header.signing {
        Signing::Unsigned => Cow::Borrowed(body),
        Signing::Signed => Cow::Owned(dechunk_authed(body)?),
    };

    let image = inflate_zone(&compressed)?;

    if image.len() < XFILE_HEADER_LEN {
        return Err(ZoneOpenError::Truncated);
    }

    let iw4_wire_format = Some(select_iw4_table(&image)?.format());
    Ok(ZoneImage {
        bytes: image,
        iw4_wire_format,
    })
}

fn dechunk_authed(body: &[u8]) -> Result<Vec<u8>, ZoneOpenError> {
    if body.len() < 8 || &body[0..8] != MAGIC_AUTH_HEADER {
        return Err(ZoneOpenError::BadAuthHeader);
    }

    let mut pos = AUTHED_CHUNK_SIZE;
    let mut out = Vec::with_capacity(body.len());
    let mut chunk_in_group = 0usize;

    while pos < body.len() {
        let end = (pos + AUTHED_CHUNK_SIZE).min(body.len());
        if chunk_in_group != 0 {
            out.extend_from_slice(&body[pos..end]);
        }
        pos = end;
        chunk_in_group = (chunk_in_group + 1) % (AUTHED_CHUNKS_PER_GROUP + 1);
    }

    Ok(out)
}

pub struct ZoneMemory {
    blocks: [Vec<u8>; crate::MAX_XFILE_COUNT],
    insert_map: Vec<u8>,
}

impl ZoneMemory {
    pub fn for_header(header: &ZoneHeader) -> ZoneMemory {
        let blocks = std::array::from_fn(|i| vec![0u8; header.block_size[i] as usize]);
        let insert_map = vec![0u8; ZoneStream::insert_map_len(header)];
        ZoneMemory { blocks, insert_map }
    }

    pub fn total_bytes(&self) -> usize {
        self.blocks.iter().map(|b| b.len()).sum()
    }

    pub fn stream<'a>(
        &'a mut self,
        image: &'a [u8],
    ) -> Result<ZoneStream<'a>, ZoneError> {
        let [b0, b1, b2, b3, b4, b5, b6, b7] = &mut self.blocks;
        let blocks: [&mut [u8]; crate::MAX_XFILE_COUNT] = [
            b0.as_mut_slice(),
            b1.as_mut_slice(),
            b2.as_mut_slice(),
            b3.as_mut_slice(),
            b4.as_mut_slice(),
            b5.as_mut_slice(),
            b6.as_mut_slice(),
            b7.as_mut_slice(),
        ];
        ZoneStream::new(image, blocks, &mut self.insert_map)
    }
}