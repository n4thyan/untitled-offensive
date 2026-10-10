//! Diagnostic: compare IW4 triangle winding against vcod winding convention.
//!
//! vcod (CoD1) convention: cross(p1-p0, p2-p0) opposes the vertex normal
//! (i.e., geometric normal points opposite to the stored normal).
//! If IW4 has the same convention, no swap is needed.
//! If IW4 has the opposite, indices 1 and 2 must be swapped in the adapter.

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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| {
            r"C:\Program Files (x86)\Steam\steamapps\common\Call of Duty Modern Warfare 2\zone\english\mp_rust.ff"
                .to_string()
        });

    let child = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || run(&path))
        .expect("failed to spawn thread");

    match child.join() {
        Ok(()) => {}
        Err(e) => {
            eprintln!("Thread panicked: {e:?}");
            std::process::exit(1);
        }
    }
}

fn run(path: &str) {
    println!("=== IW4 Winding Comparison ===\n");

    let zone = match iw4::open_zone(path) {
        Ok(z) => z,
        Err(e) => {
            eprintln!("Failed to open zone: {e}");
            std::process::exit(1);
        }
    };
    let header = zone.header().expect("failed to parse header");
    let mut memory = ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&zone.bytes).expect("failed to create stream");

    let mut capture = WorldCapture {
        gfx_world: None,
        clip_map: None,
        map_ents: None,
    };

    load_zone(&mut stream, &mut capture).expect("failed to load zone");

    let gfx = capture.gfx_world.expect("no GfxWorld");
    let clip = capture.clip_map.expect("no ClipMap");
    let map_ents = capture.map_ents.expect("no MapEnts");

    let bsp = adapter::build_bsp(&stream, &gfx, &clip, &map_ents)
        .expect("failed to build Bsp");

    // For each soup, check the winding of its triangles.
    // vcod convention: cross(p1-p0, p2-0) dot normal < 0 (opposes)
    // If IW4 matches: dot < 0
    // If IW4 is opposite: dot > 0

    let mut vcod_convention = 0usize;  // dot < 0
    let mut opposite_convention = 0usize; // dot > 0
    let mut degenerate = 0usize; // cross product near zero

    for (si, soup) in bsp.soups.iter().enumerate() {
        let fv = soup.first_vertex as usize;
        let vc = soup.vertex_count as usize;
        let fi = soup.first_index as usize;
        let ic = soup.index_count as usize;

        if fv + vc > bsp.verts.len() || fi + ic > bsp.indices.len() {
            continue;
        }

        let idx_slice = &bsp.indices[fi..fi + ic];
        for tri in idx_slice.chunks(3) {
            if tri.len() < 3 {
                continue;
            }
            let p0 = &bsp.verts[fv + tri[0] as usize];
            let p1 = &bsp.verts[fv + tri[1] as usize];
            let p2 = &bsp.verts[fv + tri[2] as usize];

            let v0 = p0.pos;
            let v1 = p1.pos;
            let v2 = p2.pos;
            let n = p0.normal;

            // cross(v1-v0, v2-v0)
            let e1 = [v1[0]-v0[0], v1[1]-v0[1], v1[2]-v0[2]];
            let e2 = [v2[0]-v0[0], v2[1]-v0[1], v2[2]-v0[2]];
            let geo = [
                e1[1]*e2[2] - e1[2]*e2[1],
                e1[2]*e2[0] - e1[0]*e2[2],
                e1[0]*e2[1] - e1[1]*e2[0],
            ];
            let geo_len_sq = geo[0]*geo[0] + geo[1]*geo[1] + geo[2]*geo[2];
            if geo_len_sq < 1e-9 {
                degenerate += 1;
                continue;
            }

            let dot = geo[0]*n[0] + geo[1]*n[1] + geo[2]*n[2];
            if dot < 0.0 {
                vcod_convention += 1;
            } else {
                opposite_convention += 1;
            }
        }
    }

    let total = vcod_convention + opposite_convention + degenerate;
    println!("Total triangles checked: {total}");
    println!("  vcod convention (dot < 0): {vcod_convention}");
    println!("  opposite convention (dot > 0): {opposite_convention}");
    println!("  degenerate: {degenerate}");

    if vcod_convention > opposite_convention {
        println!("\nCONCLUSION: IW4 winding MATCHES vcod convention.");
        println!("  No index swap needed. Backface culling should work normally.");
    } else if opposite_convention > vcod_convention {
        println!("\nCONCLUSION: IW4 winding is OPPOSITE to vcod convention.");
        println!("  Swap indices 1 and 2 in the adapter to fix winding.");
    } else {
        println!("\nCONCLUSION: Mixed or inconclusive winding.");
    }
}
