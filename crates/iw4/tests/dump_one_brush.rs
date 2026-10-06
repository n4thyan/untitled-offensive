//! Dump first N IW4 brush fields from mp_rust.ff to understand CBRUSH layout

use iw4::{AssetSink, ClipMapGeometry, GfxWorldGeometry, MapEntsGeometry, ScriptStrings, ZoneMemory, ZoneStream, load_zone, open_zone};

struct WorldCapture {
    gfx_world: Option<GfxWorldGeometry>,
    clip_map: Option<ClipMapGeometry>,
    map_ents: Option<MapEntsGeometry>,
}

impl AssetSink for WorldCapture {
    fn set_script_strings(&mut self, _strings: ScriptStrings) {}
    fn begin_assets(&mut self, _count: usize) {}
    fn load_asset(&mut self, s: &mut ZoneStream<'_>, _index: usize, ty: iw4::AssetType, slot: iw4::Ptr) -> iw4::Result<()> {
        iw4::load_asset_at(s, ty, slot)?;
        if let Some(g) = s.gfx_world() { self.gfx_world = Some(g); }
        if let Some(c) = s.clip_map() { self.clip_map = Some(c); }
        if let Some(m) = s.map_ents() { self.map_ents = Some(m); }
        Ok(())
    }
}

#[test]
fn dump_one_brush() {
    let path = r"C:\Program Files (x86)\Steam\steamapps\common\Call of Duty Modern Warfare 2\zone\english\mp_rust.ff";
    if !std::path::Path::new(path).exists() { eprintln!("SKIP"); return; }
    let zone = open_zone(std::path::Path::new(path)).unwrap();
    let header = zone.header().unwrap();
    let mut memory = ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&zone.bytes).unwrap();

    let mut capture = WorldCapture { gfx_world: None, clip_map: None, map_ents: None };
    load_zone(&mut stream, &mut capture).unwrap();
    let clip = capture.clip_map.unwrap();

    let bc = clip.brush_count;
    let brushes = clip.brushes.unwrap();

    eprintln!("brush_side_count={} brush_count={}", clip.brush_side_count, bc);

    // Read first 3 brushes: each CBRUSH is 48 bytes
    for i in 0..bc.min(3) {
        let brush = brushes.at(i * 48);
        let vals: Vec<u32> = (0..12).map(|o| stream.u32_at(brush, o * 4).unwrap()).collect();
        eprintln!("brush[{}] = {:?}", i, vals);
    }

    // Read first 5 brush sides: each CBRUSH_SIDE is 8 bytes
    let bs = clip.brush_sides.unwrap();
    for i in 0..(clip.brush_side_count.min(5)) {
        let side = bs.at(i * 8);
        let plane = stream.u32_at(side, 0).unwrap();
        let mat = stream.u32_at(side, 4).unwrap();
        eprintln!("brush_side[{}] plane={} material={}", i, plane, mat);
    }

    // Read brush_contents (4 bytes each)
    if let Some(bc_ptr) = clip.brush_contents {
        for i in 0..bc.min(3) {
            let contents = stream.u32_at(bc_ptr, i * 4).unwrap();
            eprintln!("brush_contents[{}] = 0x{:x}", i, contents);
        }
    }

    // Read brush_bounds (24 bytes each)
    if let Some(bb_ptr) = clip.brush_bounds {
        for i in 0..bc.min(2) {
            let bb = bb_ptr.at(i * 24);
            let vals: Vec<f32> = (0..6).map(|o| stream.f32_at(bb, o * 4).unwrap()).collect();
            eprintln!("brush_bounds[{}] = {:?}", i, vals);
        }
    }
}
