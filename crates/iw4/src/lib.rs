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
pub mod adapter;

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

/// Load an IW4 multiplayer fastfile into the normalized BSP consumed by both
/// the client renderer and the dedicated server. `None` means that this
/// install does not contain the requested map.
pub fn load_map_from_install(
    install: &std::path::Path,
    map: &str,
) -> anyhow::Result<Option<vcod_common::bsp::Bsp>> {
    let ff_path = install
        .join("zone")
        .join("english")
        .join(format!("{map}.ff"));
    if !ff_path.is_file() {
        return Ok(None);
    }

    let zone = open_zone(&ff_path)
        .map_err(|e| anyhow::anyhow!("cannot open {}: {e}", ff_path.display()))?;
    let header = zone
        .header()
        .map_err(|e| anyhow::anyhow!("cannot parse {} header: {e}", ff_path.display()))?;
    let mut memory = ZoneMemory::for_header(&header);
    let mut stream = memory
        .stream(&zone.bytes)
        .map_err(|e| anyhow::anyhow!("cannot create {} stream: {e:?}", ff_path.display()))?;
    let mut capture = adapter::WorldCapture::default();
    load_zone(&mut stream, &mut capture)
        .map_err(|e| anyhow::anyhow!("cannot load {}: {e:?}", ff_path.display()))?;

    let gfx = capture
        .gfx_world
        .ok_or_else(|| anyhow::anyhow!("no GfxWorld found in {}", ff_path.display()))?;
    let clip = capture
        .clip_map
        .ok_or_else(|| anyhow::anyhow!("no ClipMap found in {}", ff_path.display()))?;
    let map_ents = capture
        .map_ents
        .ok_or_else(|| anyhow::anyhow!("no MapEnts found in {}", ff_path.display()))?;
    let bsp = adapter::build_bsp_with_materials(
        &stream,
        &gfx,
        &clip,
        &map_ents,
        &capture.material_images,
    )
    .map_err(|e| anyhow::anyhow!("cannot normalize {}: {e}", ff_path.display()))?;
    Ok(Some(bsp))
}
