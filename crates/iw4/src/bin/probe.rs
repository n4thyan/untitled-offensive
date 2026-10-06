//! Probe: load mp_rust.ff and print structured world data.
//!
//! Usage: cargo run -p iw4 --bin probe -- <path-to-mp_rust.ff>

use iw4::{
    AssetSink, AssetTable, ClipMapGeometry, GfxWorldGeometry, MapEntsGeometry, ScriptStrings,
    ZoneMemory, ZoneStream, load_zone,
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

    // Run in a thread with a larger stack to handle deep recursion in load_zone
    let child = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024) // 256 MB
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
    println!("Loading: {path}");
    let zone = match iw4::open_zone(&path) {
        Ok(z) => z,
        Err(e) => {
            eprintln!("Failed to open zone: {e}");
            std::process::exit(1);
        }
    };

    let header = match zone.header() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Failed to parse header: {e}");
            std::process::exit(1);
        }
    };

    println!("Zone header:");
    println!("  size: {}", header.size);
    println!("  external_size: {}", header.external_size);
    for (i, b) in header.block_size.iter().enumerate() {
        if *b > 0 {
            println!("  block[{i}]: {b} bytes");
        }
    }

    let mut memory = ZoneMemory::for_header(&header);
    println!("  total memory: {} bytes", memory.total_bytes());

    let mut stream = match memory.stream(&zone.bytes) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to create stream: {e:?}");
            std::process::exit(1);
        }
    };

    let mut capture = WorldCapture {
        gfx_world: None,
        clip_map: None,
        map_ents: None,
    };

    let table = match iw4::load_zone(&mut stream, &mut capture) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to load zone: {e:?}");
            std::process::exit(1);
        }
    };

    println!("\nAsset table:");
    println!("  asset count: {}", table.count());

    // Print GfxWorld info
    if let Some(gfx) = capture.gfx_world {
        println!("\n=== GfxWorld ===");
        println!("  vertex_count: {}", gfx.vertex_count);
        println!("  index_count: {}", gfx.index_count);
        println!("  surface_count: {}", gfx.surface_count);
        println!("  lightmap_count: {}", gfx.lightmap_count);
        println!("  sky_count: {}", gfx.sky_count);
        println!("  cell_count: {}", gfx.cell_count);
        println!("  plane_count: {}", gfx.plane_count);
        println!("  node_count: {}", gfx.node_count);
        println!("  portal_count: {}", gfx.portal_count);
        println!("  aabb_node_count: {}", gfx.aabb_node_count);
        println!("  smodel_count: {}", gfx.smodel_count);
        println!("  static_surface_count: {}", gfx.static_surface_count);
        println!("  model_count: {}", gfx.model_count);
        if let Some(bounds) = gfx.bounds {
            println!("  bounds: {:?}", bounds);
        }
    } else {
        println!("\n!!! No GfxWorld found in zone");
    }

    // Print ClipMap info
    if let Some(clip) = capture.clip_map {
        println!("\n=== ClipMap ===");
        println!("  plane_count: {}", clip.plane_count);
        println!("  static_model_count: {}", clip.static_model_count);
        println!("  material_count: {}", clip.material_count);
        println!("  brush_side_count: {}", clip.brush_side_count);
        println!("  node_count: {}", clip.node_count);
        println!("  leaf_count: {}", clip.leaf_count);
        println!("  leafbrush_node_count: {}", clip.leafbrush_node_count);
        println!("  brush_count: {}", clip.brush_count);
        println!("  cmodel_count: {}", clip.cmodel_count);
        println!("  vert_count: {}", clip.vert_count);
        println!("  tri_count: {}", clip.tri_count);
        println!("  partition_count: {}", clip.partition_count);
        println!("  aabb_tree_count: {}", clip.aabb_tree_count);
    } else {
        println!("\n!!! No ClipMap found in zone");
    }

    // Print MapEnts info
    if let Some(ents) = capture.map_ents {
        println!("\n=== MapEnts ===");
        println!("  entity_chars: {}", ents.entity_chars);
        println!("  trigger_model_count: {}", ents.trigger_model_count);
        println!("  trigger_hull_count: {}", ents.trigger_hull_count);
        println!("  trigger_slab_count: {}", ents.trigger_slab_count);
    } else {
        println!("\n!!! No MapEnts found in zone");
    }

    println!("\nProbe complete.");
}