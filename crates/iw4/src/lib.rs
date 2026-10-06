#![forbid(unsafe_code)]

mod asset_type;
mod content;
mod envelope;
mod load;
pub mod shader;
mod stream;
mod wire;
mod zone;
pub mod zone_open;

pub use wire::{Iw4WireFormat, WireAssetTable, WirePointer, WireTableError, read_wire_asset_table};

pub use asset_type::AssetType;
pub use content::{
    AssetSink, AssetTable, ScriptStrings, XASSET_ENTRY_LEN, XASSET_LIST_LEN, load_zone,
    open_asset_table,
};
pub use envelope::{
    FILE_PREAMBLE_LEN, FileHeader, FileHeaderError, MAGIC_AUTH_HEADER, MAGIC_SIGNED,
    MAGIC_UNSIGNED, Signing, ZONE_VERSION_PC, parse_file_header,
};
pub use load::{
    AssetLinkSink, FontCapture, GlyphCapture, MenuDefCapture, MenuItemLayout, MenuRectCapture,
    MenuScriptKind, load_asset_at, load_asset_at_observed, load_asset_at_with, load_asset_body,
};
pub use stream::{
    alloc_stream_pos, convert_offset_to_alias, convert_offset_to_pointer, inc_stream_pos,
    insert_pointer, load_stream, pop_stream_pos, push_stream_pos,
};
pub use zone::*;
pub use zone_open::{ZoneImage, ZoneMemory, ZoneOpenError, open_zone};
