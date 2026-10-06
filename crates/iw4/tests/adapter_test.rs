//! Integration test: load real mp_rust.ff and convert to vcod Bsp
//!
//! This test requires a local MW2 install. It is skipped automatically
//! when the file is not present.

use iw4::{
    adapter, AssetSink, ClipMapGeometry, GfxWorldGeometry, MapEntsGeometry, ScriptStrings,
    ZoneMemory, ZoneStream, load_zone, open_zone,
};

struct WorldCapture {
    gfx_world: Option<GfxWorldGeometry>,
    clip_map: Option<ClipMapGeometry>,
    map_ents: Option<MapEntsGeometry>,
}

impl AssetSink for WorldCapture {
    fn set_script_strings(&mut self, _strings: ScriptStrings) {}
    fn begin_assets(&mut self, _count: usize) {}
    fn load_asset(
        &mut self,
        s: &mut ZoneStream<'_>,
        _index: usize,
        ty: iw4::AssetType,
        slot: iw4::Ptr,
    ) -> iw4::Result<()> {
        iw4::load_asset_at(s, ty, slot)?;
        if let Some(g) = s.gfx_world() {
            self.gfx_world = Some(g);
        }
        if let Some(c) = s.clip_map() {
            self.clip_map = Some(c);
        }
        if let Some(m) = s.map_ents() {
            self.map_ents = Some(m);
        }
        Ok(())
    }
}

fn find_mp_rust() -> Option<std::path::PathBuf> {
    let candidates = [
        r"C:\Program Files (x86)\Steam\steamapps\common\Call of Duty Modern Warfare 2\zone\english\mp_rust.ff",
        r"C:\Program Files\Steam\steamapps\common\Call of Duty Modern Warfare 2\zone\english\mp_rust.ff",
        r"D:\Steam\steamapps\common\Call of Duty Modern Warfare 2\zone\english\mp_rust.ff",
        r"E:\Steam\steamapps\common\Call of Duty Modern Warfare 2\zone\english\mp_rust.ff",
    ];
    for c in &candidates {
        let p = std::path::Path::new(c);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    None
}

#[test]
fn mp_rust_loads_and_converts_to_bsp() {
    let path = match find_mp_rust() {
        Some(p) => p,
        None => {
            eprintln!("SKIP: mp_rust.ff not found (no local MW2 install)");
            return;
        }
    };

    let zone = open_zone(&path).expect("failed to open mp_rust.ff");
    let header = zone.header().expect("failed to parse header");

    // Verify zone header is reasonable
    assert!(header.size > 0);
    assert!(header.external_size > 0);

    let mut memory = ZoneMemory::for_header(&header);
    let mut stream = memory
        .stream(&zone.bytes)
        .expect("failed to create zone stream");

    let mut capture = WorldCapture {
        gfx_world: None,
        clip_map: None,
        map_ents: None,
    };

    let table = load_zone(&mut stream, &mut capture).expect("failed to load zone");
    assert!(table.count() > 0, "zone has no assets");

    // Verify we captured the key world data
    let gfx = capture
        .gfx_world
        .expect("no GfxWorld found in mp_rust.ff");
    let clip = capture
        .clip_map
        .expect("no ClipMap found in mp_rust.ff");
    let map_ents = capture
        .map_ents
        .expect("no MapEnts found in mp_rust.ff");

    // Verify GfxWorld has real geometry
    assert!(gfx.vertex_count > 0, "GfxWorld has no vertices");
    assert!(gfx.index_count > 0, "GfxWorld has no indices");
    assert!(gfx.surface_count > 0, "GfxWorld has no surfaces");
    assert!(gfx.smodel_count > 0, "GfxWorld has no static models");

    // Verify ClipMap has real collision data
    assert!(clip.plane_count > 0, "ClipMap has no planes");
    assert!(clip.brush_count > 0, "ClipMap has no brushes");
    assert!(clip.vert_count > 0, "ClipMap has no verts");
    assert!(clip.tri_count > 0, "ClipMap has no tris");

    // Verify MapEnts has entity data
    assert!(map_ents.entity_chars > 0, "MapEnts has no entity chars");

    // Convert to vcod Bsp
    let bsp = adapter::build_bsp(&stream, &gfx, &clip, &map_ents)
        .expect("failed to build Bsp from IW4 data");

    // Verify the Bsp has real geometry
    assert!(!bsp.verts.is_empty(), "Bsp has no vertices");
    assert!(!bsp.indices.is_empty(), "Bsp has no indices");
    assert!(!bsp.soups.is_empty(), "Bsp has no soups");
    assert!(!bsp.materials.is_empty(), "Bsp has no materials");
    assert!(!bsp.planes.is_empty(), "Bsp has no planes");
    assert!(!bsp.brushes.is_empty(), "Bsp has no brushes");
    assert!(!bsp.collision_verts.is_empty(), "Bsp has no collision verts");
    assert!(!bsp.collision_indices.is_empty(), "Bsp has no collision indices");
    assert!(!bsp.entities.is_empty(), "Bsp has no entities");
    assert!(!bsp.models.is_empty(), "Bsp has no models");

    // Print summary for test output
    println!("mp_rust.ff loaded successfully:");
    println!("  GfxWorld: {} verts, {} indices, {} surfaces, {} static models",
        gfx.vertex_count, gfx.index_count, gfx.surface_count, gfx.smodel_count);
    println!("  ClipMap: {} planes, {} brushes, {} verts, {} tris",
        clip.plane_count, clip.brush_count, clip.vert_count, clip.tri_count);
    println!("  MapEnts: {} entity chars", map_ents.entity_chars);
    println!("  Bsp: {} verts, {} indices, {} soups, {} materials",
        bsp.verts.len(), bsp.indices.len(), bsp.soups.len(), bsp.materials.len());
    println!("  Bsp collision: {} planes, {} brushes, {} verts, {} indices",
        bsp.planes.len(), bsp.brushes.len(), bsp.collision_verts.len(), bsp.collision_indices.len());
}