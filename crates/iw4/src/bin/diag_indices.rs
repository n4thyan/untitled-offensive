//! Diagnostic: prove index semantics and find stretched geometry.
//!
//! Usage: cargo run -p iw4 --bin diag_indices -- <path-to-mp_rust.ff>

use iw4::{
    adapter, AssetSink, ClipMapGeometry, GfxWorldGeometry, MapEntsGeometry, ScriptStrings,
    ZoneMemory, ZoneStream, load_zone, open_zone,
};
use vcod_common::bsp::TriangleSoup;

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
    println!("=== IW4 Index Semantics + Geometry Diagnostic ===\n");

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

    println!("Loaded: {} verts, {} indices, {} soups",
        bsp.verts.len(), bsp.indices.len(), bsp.soups.len());

    // ================================================================
    // TASK 1: Prove index semantics
    // ================================================================
    println!("\n--- TASK 1: Index Semantics ---");

    let mut local_valid = 0usize;
    let mut global_valid = 0usize;
    let mut both_valid = 0usize;
    let mut invalid = 0usize;
    let mut invalid_examples: Vec<(usize, &TriangleSoup)> = Vec::new();

    for (i, soup) in bsp.soups.iter().enumerate() {
        let fv = soup.first_vertex as usize;
        let vc = soup.vertex_count as usize;
        let fi = soup.first_index as usize;
        let ic = soup.index_count as usize;

        if fi + ic > bsp.indices.len() {
            invalid += 1;
            if invalid_examples.len() < 5 {
                invalid_examples.push((i, soup));
            }
            continue;
        }

        let idx_slice = &bsp.indices[fi..fi + ic];
        let is_local = idx_slice.iter().all(|&idx| (idx as usize) < vc);
        let is_global = idx_slice.iter().all(|&idx| {
            let g = idx as usize;
            g >= fv && g < fv + vc
        });

        if is_local && is_global {
            both_valid += 1;
        } else if is_local {
            local_valid += 1;
        } else if is_global {
            global_valid += 1;
        } else {
            invalid += 1;
            if invalid_examples.len() < 5 {
                invalid_examples.push((i, soup));
            }
        }
    }

    let total = bsp.soups.len();
    println!("total soups: {total}");
    println!("local_valid: {local_valid}");
    println!("global_valid: {global_valid}");
    println!("both_valid: {both_valid}");
    println!("invalid: {invalid}");

    if !invalid_examples.is_empty() {
        println!("\nFirst invalid soups:");
        for (i, soup) in &invalid_examples {
            let fi = soup.first_index as usize;
            let ic = soup.index_count as usize;
            let idx_slice = &bsp.indices[fi..(fi + ic).min(bsp.indices.len())];
            let min_idx = idx_slice.iter().copied().min().unwrap_or(0);
            let max_idx = idx_slice.iter().copied().max().unwrap_or(0);
            println!("  soup {i}: first_vertex={} vertex_count={} first_index={} index_count={} min_idx={} max_idx={}",
                soup.first_vertex, soup.vertex_count, soup.first_index, soup.index_count,
                min_idx, max_idx);
        }
    }

    // Print 5 representative soups
    println!("\nRepresentative soups (first 5):");
    for (i, soup) in bsp.soups.iter().take(5).enumerate() {
        let fi = soup.first_index as usize;
        let ic = soup.index_count as usize;
        let idx_slice = &bsp.indices[fi..(fi + ic).min(bsp.indices.len())];
        let min_idx = idx_slice.iter().copied().min().unwrap_or(0);
        let max_idx = idx_slice.iter().copied().max().unwrap_or(0);
        println!("  soup {i}: first_vertex={} vertex_count={} first_index={} index_count={} min_idx={} max_idx={}",
            soup.first_vertex, soup.vertex_count, soup.first_index, soup.index_count,
            min_idx, max_idx);
    }

    // ================================================================
    // TASK 2: Find stretched geometry
    // ================================================================
    println!("\n--- TASK 2: Vertex Bounds Analysis ---");

    let mut min_pos = [f32::INFINITY; 3];
    let mut max_pos = [f32::NEG_INFINITY; 3];
    let mut count_gt_10k = 0usize;
    let mut count_gt_20k = 0usize;
    let mut count_gt_50k = 0usize;

    for v in &bsp.verts {
        for a in 0..3 {
            min_pos[a] = min_pos[a].min(v.pos[a]);
            max_pos[a] = max_pos[a].max(v.pos[a]);
        }
        let max_abs = v.pos.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        if max_abs > 10000.0 {
            count_gt_10k += 1;
        }
        if max_abs > 20000.0 {
            count_gt_20k += 1;
        }
        if max_abs > 50000.0 {
            count_gt_50k += 1;
        }
    }

    println!("world bounds: mins={min_pos:?} maxs={max_pos:?}");
    println!("vertices with |coord| > 10000: {count_gt_10k}");
    println!("vertices with |coord| > 20000: {count_gt_20k}");
    println!("vertices with |coord| > 50000: {count_gt_50k}");

    // Find soups referencing the most extreme vertices
    let mut extreme_soups: Vec<(usize, f32, [f32; 3], [f32; 3])> = Vec::new();
    for (si, soup) in bsp.soups.iter().enumerate() {
        let fv = soup.first_vertex as usize;
        let vc = soup.vertex_count as usize;
        if fv + vc > bsp.verts.len() {
            continue;
        }
        let mut soup_min = [f32::INFINITY; 3];
        let mut soup_max = [f32::NEG_INFINITY; 3];
        for v in &bsp.verts[fv..fv + vc] {
            for a in 0..3 {
                soup_min[a] = soup_min[a].min(v.pos[a]);
                soup_max[a] = soup_max[a].max(v.pos[a]);
            }
        }
        let max_abs = soup_min.iter().chain(soup_max.iter())
            .map(|x| x.abs())
            .fold(0.0f32, f32::max);
        extreme_soups.push((si, max_abs, soup_min, soup_max));
    }

    extreme_soups.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    println!("\nTop 5 soups by extreme vertex magnitude:");
    for (si, max_abs, smin, smax) in extreme_soups.iter().take(5) {
        let soup = &bsp.soups[*si];
        let fi = soup.first_index as usize;
        let ic = soup.index_count as usize;
        let idx_slice = &bsp.indices[fi..(fi + ic).min(bsp.indices.len())];
        let min_idx = idx_slice.iter().copied().min().unwrap_or(0);
        let max_idx = idx_slice.iter().copied().max().unwrap_or(0);
        println!("  soup {si}: max_abs={max_abs:.1} AABB=[{smin:?}, {smax:?}] first_vertex={} vertex_count={} first_index={} index_count={} idx_range=[{min_idx}, {max_idx}]",
            soup.first_vertex, soup.vertex_count, soup.first_index, soup.index_count);
    }

    // Also show the most common vertex magnitude range
    let mut normal_max = 0.0f32;
    for v in &bsp.verts {
        let max_abs = v.pos.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        if max_abs < 50000.0 && max_abs > normal_max {
            normal_max = max_abs;
        }
    }
    println!("\nLargest 'normal' vertex magnitude (< 50000): {normal_max:.1}");

    println!("\n=== Diagnostic complete ===");
}
