//! Diagnostic: compare declared-range AABB vs referenced-only AABB for suspicious soups.
//!
//! For soups sharing first_vertex=107915, vertex_count=57051, calculate:
//! A) declared-range AABB (all verts in first_vertex..first_vertex+vertex_count)
//! B) referenced-only AABB (only verts actually indexed by the soup)
//!
//! Usage: cargo run -p iw4 --bin diag_bounds -- <path-to-mp_rust.ff>

use iw4::{
    adapter, AssetSink, ClipMapGeometry, GfxWorldGeometry, MapEntsGeometry,
    Ptr, ScriptStrings, ZoneMemory, ZonePtr, ZoneStream, load_zone, open_zone,
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
    println!("=== Referenced-Vertex Bounds Diagnostic ===\n");

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

    println!("Loaded: {} verts, {} indices, {} soups\n",
        bsp.verts.len(), bsp.indices.len(), bsp.soups.len());

    // Find soups with the largest declared vertex ranges
    let mut soup_declared_sizes: Vec<(usize, u32, u32)> = bsp.soups.iter().enumerate()
        .map(|(i, s)| (i, s.first_vertex, s.vertex_count as u32))
        .collect();
    soup_declared_sizes.sort_by(|a, b| b.2.cmp(&a.2));

    // Find soups sharing the same first_vertex as the worst
    let worst_fv = soup_declared_sizes[0].1;
    let worst_vc = soup_declared_sizes[0].2;
    let sharing: Vec<usize> = bsp.soups.iter().enumerate()
        .filter(|(_, s)| s.first_vertex == worst_fv && s.vertex_count as u32 == worst_vc)
        .map(|(i, _)| i)
        .collect();

    println!("Worst declared range: first_vertex={}, vertex_count={}", worst_fv, worst_vc);
    println!("Soups sharing this exact range: {} soups", sharing.len());
    println!("Soup indices: {:?}\n", &sharing[..sharing.len().min(20)]);

    // For the 5 worst soups, calculate both AABBs
    println!("--- Top 5 soups by declared vertex_count ---");
    for (si, fv, vc) in soup_declared_sizes.iter().take(5) {
        let soup = &bsp.soups[*si];
        let fi = soup.first_index as usize;
        let ic = soup.index_count as usize;

        // Declared-range AABB
        let mut decl_min = [f32::INFINITY; 3];
        let mut decl_max = [f32::NEG_INFINITY; 3];
        let fv_usize = *fv as usize;
        let vc_usize = *vc as usize;
        if fv_usize + vc_usize <= bsp.verts.len() {
            for v in &bsp.verts[fv_usize..fv_usize + vc_usize] {
                for a in 0..3 {
                    decl_min[a] = decl_min[a].min(v.pos[a]);
                    decl_max[a] = decl_max[a].max(v.pos[a]);
                }
            }
        }

        // Referenced-only AABB
        let mut ref_min = [f32::INFINITY; 3];
        let mut ref_max = [f32::NEG_INFINITY; 3];
        let mut min_local = u16::MAX;
        let mut max_local = 0u16;
        let mut unique_indices = std::collections::HashSet::new();
        let mut max_abs_ref = 0.0f32;

        if fi + ic <= bsp.indices.len() {
            for &local_idx in &bsp.indices[fi..fi + ic] {
                let local = local_idx as usize;
                min_local = min_local.min(local_idx);
                max_local = max_local.max(local_idx);
                unique_indices.insert(local_idx);

                let global = fv_usize + local;
                if global < bsp.verts.len() {
                    let v = &bsp.verts[global];
                    for a in 0..3 {
                        ref_min[a] = ref_min[a].min(v.pos[a]);
                        ref_max[a] = ref_max[a].max(v.pos[a]);
                        max_abs_ref = max_abs_ref.max(v.pos[a].abs());
                    }
                }
            }
        }

        let decl_size = [
            decl_max[0] - decl_min[0],
            decl_max[1] - decl_min[1],
            decl_max[2] - decl_min[2],
        ];
        let ref_size = [
            ref_max[0] - ref_min[0],
            ref_max[1] - ref_min[1],
            ref_max[2] - ref_min[2],
        ];

        println!("\nSoup {si}:");
        println!("  first_vertex={} vertex_count={} first_index={} index_count={}",
            fv, vc, soup.first_index, soup.index_count);
        println!("  local index range: [{}, {}] ({} unique)",
            min_local, max_local, unique_indices.len());
        println!("  declared-range AABB: mins={:?} maxs={:?}", decl_min, decl_max);
        println!("  declared-range size: [{:.1}, {:.1}, {:.1}]", decl_size[0], decl_size[1], decl_size[2]);
        println!("  referenced-only AABB: mins={:?} maxs={:?}", ref_min, ref_max);
        println!("  referenced-only size: [{:.1}, {:.1}, {:.1}]", ref_size[0], ref_size[1], ref_size[2]);
        println!("  max abs referenced coord: {:.1}", max_abs_ref);
    }

    // Also check: how many soups have vertex_count > 10000?
    let huge_vc = bsp.soups.iter().filter(|s| s.vertex_count as usize > 10000).count();
    println!("\n--- Summary ---");
    println!("Soups with vertex_count > 10000: {huge_vc}");
    println!("Soups with vertex_count > 50000: {}", bsp.soups.iter().filter(|s| s.vertex_count as usize > 50000).count());

    // Check if the sharing soups have disjoint index slices
    if sharing.len() > 1 {
        println!("\n--- Index slice overlap check for sharing soups ---");
        let mut all_ranges: Vec<(usize, u32, u32)> = sharing.iter()
            .map(|&si| (si, bsp.soups[si].first_index, bsp.soups[si].index_count as u32))
            .collect();
        all_ranges.sort_by_key(|r| r.1);
        for (si, fi, ic) in all_ranges.iter().take(10) {
            println!("  soup {si}: first_index={} index_count={}", fi, ic);
        }
        // Check overlap
        let mut overlaps = 0;
        for i in 0..all_ranges.len() {
            for j in (i+1)..all_ranges.len() {
                let (si1, fi1, ic1) = all_ranges[i];
                let (si2, fi2, ic2) = all_ranges[j];
                if fi1 < fi2 + ic2 && fi2 < fi1 + ic1 {
                    overlaps += 1;
                    if overlaps <= 3 {
                        let end1 = fi1 + ic1;
                        let end2 = fi2 + ic2;
                        println!("  OVERLAP: soup {si1} [{fi1}, {end1}) and soup {si2} [{fi2}, {end2})");
                    }
                }
            }
        }
        println!("  Total overlapping pairs: {}", overlaps);
    }

    // ================================================================
    // NEW: Compute referenced-only bounds for ALL soups and rank extremes
    // ================================================================
    println!("\n=== Referenced-Only Extreme Surface Diagnostic ===\n");

    let mut soup_bounds: Vec<(usize, [f32; 3], [f32; 3], f32, f32, usize)> = Vec::new();

    for (si, soup) in bsp.soups.iter().enumerate() {
        let fv = soup.first_vertex as usize;
        let fi = soup.first_index as usize;
        let ic = soup.index_count as usize;

        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut unique = std::collections::HashSet::new();
        let mut max_abs = 0.0f32;

        if fi + ic <= bsp.indices.len() {
            for &local_idx in &bsp.indices[fi..fi + ic] {
                let global = fv + local_idx as usize;
                if global < bsp.verts.len() {
                    let v = &bsp.verts[global];
                    for a in 0..3 {
                        min[a] = min[a].min(v.pos[a]);
                        max[a] = max[a].max(v.pos[a]);
                        max_abs = max_abs.max(v.pos[a].abs());
                    }
                    unique.insert(local_idx);
                }
            }
        }

        let extent = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
        let max_extent = extent.iter().cloned().fold(0.0f32, f32::max);

        soup_bounds.push((si, min, max, max_abs, max_extent, unique.len()));
    }

    // Helper to read surface metadata
    // GfxSurface layout (24 bytes x86, 32 bytes x64):
    //   offset 0: raw_field_0 (u32)
    //   offset 4: first_vertex (u32) -- proven by adapter
    //   offset 8: vertex_count (u16) -- proven by adapter
    //   offset 10: tri_count (u16) -- proven by adapter
    //   offset 12: first_index (u32) -- proven by adapter
    //   offset 16: raw_field_16 (ptr)
    //   offset 20: raw_field_20 (u16)
    let read_surf_meta = |si: usize| -> (u32, u32, Option<ZonePtr>, u16) {
        if let Some(surfaces_ptr) = gfx.surfaces {
            let stride = stream.layout(24, 32);
            let surf = surfaces_ptr.at(si * stride);
            let raw0 = stream.u32_at(surf, 0).unwrap_or(0);
            let first_vertex = stream.u32_at(surf, 4).unwrap_or(0);
            let material_ptr = stream.ptr_at(surf, 16).ok();
            let raw20 = stream.u16_at(surf, 20).unwrap_or(0);
            (raw0, first_vertex, material_ptr, raw20)
        } else {
            (0, 0, None, 0)
        }
    };

    // Ranking A: top 10 by max absolute referenced coordinate
    let mut by_max_abs = soup_bounds.clone();
    by_max_abs.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap());

    println!("--- Top 10 by max absolute referenced coordinate ---");
    for (si, min, max, max_abs, max_extent, unique) in by_max_abs.iter().take(10) {
        let soup = &bsp.soups[*si];
        let (raw0, fv, mat_ptr, raw20) = read_surf_meta(*si);
        println!("\nSoup {si}:");
        println!("  first_vertex={} vertex_count={} first_index={} index_count={}",
            soup.first_vertex, soup.vertex_count, soup.first_index, soup.index_count);
        println!("  unique referenced vertices: {unique}");
        println!("  referenced mins: [{:.1}, {:.1}, {:.1}]", min[0], min[1], min[2]);
        println!("  referenced maxs: [{:.1}, {:.1}, {:.1}]", max[0], max[1], max[2]);
        println!("  max abs coordinate: {max_abs:.1}");
        println!("  max extent: {max_extent:.1}");
        println!("  raw_field_0: {raw0}");
        println!("  raw_field_16: {:?}", mat_ptr);
        println!("  raw_field_20: {raw20}");
    }

    // Ranking B: top 10 by referenced AABB extent
    let mut by_extent = soup_bounds.clone();
    by_extent.sort_by(|a, b| b.4.partial_cmp(&a.4).unwrap());

    println!("\n--- Top 10 by referenced AABB extent ---");
    for (si, min, max, max_abs, max_extent, unique) in by_extent.iter().take(10) {
        let soup = &bsp.soups[*si];
        let (raw0, fv, mat_ptr, raw20) = read_surf_meta(*si);
        println!("\nSoup {si}:");
        println!("  first_vertex={} vertex_count={} first_index={} index_count={}",
            soup.first_vertex, soup.vertex_count, soup.first_index, soup.index_count);
        println!("  unique referenced vertices: {unique}");
        println!("  referenced mins: [{:.1}, {:.1}, {:.1}]", min[0], min[1], min[2]);
        println!("  referenced maxs: [{:.1}, {:.1}, {:.1}]", max[0], max[1], max[2]);
        println!("  max abs coordinate: {max_abs:.1}");
        println!("  max extent: {max_extent:.1}");
        println!("  raw_field_0: {raw0}");
        println!("  raw_field_16: {:?}", mat_ptr);
        println!("  raw_field_20: {raw20}");
    }

    // Summary statistics
    let total_soups = soup_bounds.len();
    let soups_with_extreme = soup_bounds.iter().filter(|(_, _, _, max_abs, _, _)| *max_abs > 10000.0).count();
    let soups_with_large_extent = soup_bounds.iter().filter(|(_, _, _, _, max_extent, _)| *max_extent > 10000.0).count();

    println!("\n--- Referenced-Only Summary ---");
    println!("Total soups: {total_soups}");
    println!("Soups with max_abs > 10000: {soups_with_extreme}");
    println!("Soups with max_extent > 10000: {soups_with_large_extent}");

    // World bounds from referenced-only
    let mut world_min = [f32::INFINITY; 3];
    let mut world_max = [f32::NEG_INFINITY; 3];
    for (_, min, max, _, _, _) in &soup_bounds {
        for a in 0..3 {
            world_min[a] = world_min[a].min(min[a]);
            world_max[a] = world_max[a].max(max[a]);
        }
    }
    println!("Referenced-only world bounds: mins={world_min:?} maxs={world_max:?}");
    println!("Referenced-only world size: [{:.1}, {:.1}, {:.1}]",
        world_max[0] - world_min[0],
        world_max[1] - world_min[1],
        world_max[2] - world_min[2]
    );

    println!("\n=== Diagnostic complete ===");
}
