//! Verify brush side pointer interpretation

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
fn verify_brush_pointers() {
    let path = r"C:\Program Files (x86)\Steam\steamapps\common\Call of Duty Modern Warfare 2\zone\english\mp_rust.ff";
    if !std::path::Path::new(path).exists() { eprintln!("SKIP"); return; }
    let zone = open_zone(std::path::Path::new(path)).unwrap();
    let header = zone.header().unwrap();
    let mut memory = ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&zone.bytes).unwrap();

    let mut capture = WorldCapture { gfx_world: None, clip_map: None, map_ents: None };
    load_zone(&mut stream, &mut capture).unwrap();
    let clip = capture.clip_map.unwrap();

    let brushes = clip.brushes.unwrap();
    let brush_sides = clip.brush_sides.unwrap();
    
    eprintln!("brush_count={} brush_side_count={}", clip.brush_count, clip.brush_side_count);
    
    // Read first 5 brushes and their side pointers
    for i in 0..5 {
        let brush = brushes.at(i * 48);
        let val4 = stream.u32_at(brush, 4).unwrap();
        let val8 = stream.u32_at(brush, 8).unwrap();
        let val12 = stream.u32_at(brush, 12).unwrap();
        let val16 = stream.u32_at(brush, 16).unwrap();
        let val20 = stream.u32_at(brush, 20).unwrap();
        let val24 = stream.u32_at(brush, 24).unwrap();
        let val28 = stream.u32_at(brush, 28).unwrap();
        let val32 = stream.u32_at(brush, 32).unwrap();
        let val36 = stream.u32_at(brush, 36).unwrap();
        let val40 = stream.u32_at(brush, 40).unwrap();
        let val44 = stream.u32_at(brush, 44).unwrap();
        eprintln!("brush[{}] @4={} @8={} @12={} @16={} @20={} @24={} @28={} @32={} @36={} @40={} @44={}",
            i, val4, val8, val12, val16, val20, val24, val28, val32, val36, val40, val44);
    }
    
    // Try interpreting val4 as an index into brush_sides
    eprintln!("\nInterpreting @4 as brush_side index:");
    for i in 0..5 {
        let brush = brushes.at(i * 48);
        let side_idx = stream.u32_at(brush, 4).unwrap() as usize;
        if side_idx < clip.brush_side_count {
            let side = brush_sides.at(side_idx * 8);
            let plane = stream.u32_at(side, 0).unwrap();
            eprintln!("brush[{}] side_idx={} plane={}", i, side_idx, plane);
        } else {
            eprintln!("brush[{}] side_idx={} OUT OF RANGE (max {})", i, side_idx, clip.brush_side_count - 1);
        }
    }
    
    // Also check brush_contents
    eprintln!("\nbrush_contents:");
    if let Some(bc) = clip.brush_contents {
        for i in 0..5 {
            let contents = stream.u32_at(bc, i * 4).unwrap();
            eprintln!("  [{}] = 0x{:x}", i, contents);
        }
    }
    
    // Check brush_bounds
    eprintln!("\nbrush_bounds:");
    if let Some(bb) = clip.brush_bounds {
        for i in 0..3 {
            let bb_ptr = bb.at(i * 24);
            let mins: [f32; 3] = [stream.f32_at(bb_ptr, 0).unwrap(), stream.f32_at(bb_ptr, 4).unwrap(), stream.f32_at(bb_ptr, 8).unwrap()];
            let maxs: [f32; 3] = [stream.f32_at(bb_ptr, 12).unwrap(), stream.f32_at(bb_ptr, 16).unwrap(), stream.f32_at(bb_ptr, 20).unwrap()];
            eprintln!("  [{}] min={:?} max={:?}", i, mins, maxs);
        }
    }
}