//! Adapter: IW4 GfxWorld/ClipMap/MapEnts -> vcod Bsp
//!
//! Converts parsed MW2 world data into the vcod Bsp representation
//! so the existing renderer, collision, and spawn systems can consume it.

use crate::{
    ClipMapGeometry, GfxWorldGeometry, MapEntsGeometry, Result, ZoneStream,
};
use vcod_common::bsp::{
    AabbNode, Bsp, Brush, BrushSide, Cell, DrawVert, Material, Plane, TriangleSoup,
};

/// Sanity limits for IW4 geometry fields.
/// Based on verified mp_rust.ff metrics.
const MAX_VERTICES: usize = 1_000_000;
const MAX_INDICES: usize = 2_000_000;
const MAX_SURFACES: usize = 20_000;
const MAX_PLANES: usize = 5_000;
const MAX_BRUSH_SIDES: usize = 10_000;
const MAX_BRUSHES: usize = 5_000;
const MAX_VERTS: usize = 500_000;
const MAX_TRIS: usize = 1_000_000;
const MAX_ENTITY_CHARS: usize = 200_000;

/// Errors from the IW4->Bsp adapter
#[derive(Debug)]
pub enum AdapterError {
    Zone(crate::ZoneError),
    InvalidGeometry(String),
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Zone(e) => write!(f, "zone error: {e}"),
            Self::InvalidGeometry(s) => write!(f, "invalid geometry: {s}"),
        }
    }
}

impl std::error::Error for AdapterError {}

fn checked_usize(v: u32, name: &str) -> Result<usize> {
    let v = v as usize;
    if v > MAX_VERTICES {
        return Err(crate::ZoneError::Truncated {
            at: 0,
            needed: v,
            len: v,
        });
    }
    Ok(v)
}

fn checked_usize64(v: usize, name: &str) -> Result<usize> {
    if v > MAX_VERTICES {
        return Err(crate::ZoneError::Truncated {
            at: 0,
            needed: v,
            len: v,
        });
    }
    Ok(v)
}

/// Convert a parsed GfxWorld into vcod draw geometry (verts, indices, soups, materials)
pub fn gfxworld_to_draw_geometry(
    s: &ZoneStream<'_>,
    gfx: &GfxWorldGeometry,
) -> Result<(Vec<DrawVert>, Vec<u16>, Vec<TriangleSoup>, Vec<Material>)> {
    // Sanity checks on counts
    let vertex_count = checked_usize(gfx.vertex_count as u32, "vertex_count")?;
    let index_count = checked_usize(gfx.index_count as u32, "index_count")?;
    let surface_count = checked_usize(gfx.surface_count as u32, "surface_count")?;

    if vertex_count > MAX_VERTICES {
        return Err(crate::ZoneError::Truncated {
            at: 0,
            needed: vertex_count,
            len: vertex_count,
        });
    }
    if index_count > MAX_INDICES {
        return Err(crate::ZoneError::Truncated {
            at: 0,
            needed: index_count,
            len: index_count,
        });
    }
    if surface_count > MAX_SURFACES {
        return Err(crate::ZoneError::Truncated {
            at: 0,
            needed: surface_count,
            len: surface_count,
        });
    }

    // Read vertices: 44 bytes each, same layout as vcod DrawVert
    let verts = if let Some(vptr) = gfx.vertices {
        let mut verts = Vec::with_capacity(vertex_count);
        for i in 0..vertex_count {
            let base = vptr.at(i * 44);
            let pos = [
                s.f32_at(base, 0)?,
                s.f32_at(base, 4)?,
                s.f32_at(base, 8)?,
            ];
            let normal = [
                s.f32_at(base, 12)?,
                s.f32_at(base, 16)?,
                s.f32_at(base, 20)?,
            ];
            let uv = [
                s.f32_at(base, 24)?,
                s.f32_at(base, 28)?,
            ];
            let color = [
                s.u8_at(base, 32)?,
                s.u8_at(base, 33)?,
                s.u8_at(base, 34)?,
                s.u8_at(base, 35)?,
            ];
            let lm_uv = [
                s.f32_at(base, 36)?,
                s.f32_at(base, 40)?,
            ];
            verts.push(DrawVert {
                pos,
                uv,
                lm_uv,
                normal,
                color,
            });
        }
        verts
    } else {
        Vec::new()
    };

    // Read indices: u16 each
    let indices = if let Some(iptr) = gfx.indices {
        let mut indices = Vec::with_capacity(index_count);
        for i in 0..index_count {
            indices.push(s.u16_at(iptr, i * 2)?);
        }
        indices
    } else {
        Vec::new()
    };

    // Read surfaces and build soups.
    // GfxSurface is 24 bytes on x86:
    //   offset 0: sort (u32)
    //   offset 4: flags (u32)
    //   offset 8: first_vertex (u32)
    //   offset 12: vertex_count (u32) -- but actually u16 in vcod
    //   offset 16: material pointer (ptr)
    //   offset 20: lightmap index (u16) + pad
    //
    // vcod TriangleSoup: material(u16), lightmap(u16), first_vertex(u32),
    //   vertex_count(u16), first_index(u32), index_count(u16)
    //
    // For the FIRST milestone, use ONE fallback material for all surfaces.
    let mut soups = Vec::new();
    let fallback_material = Material {
        name: "iw4/fallback".to_string(),
        surface_flags: 0,
        content_flags: 0,
    };
    let materials = vec![fallback_material];

    if let Some(sptr) = gfx.surfaces {
        let stride = s.layout(24, 32);
        for i in 0..surface_count {
            let surf = sptr.at(i * stride);
            let first_vertex = s.u32_at(surf, s.layout(4, 4))?;
            let surf_vertex_count = s.u16_at(surf, s.layout(8, 8))?;
            let surf_tri_count = s.u16_at(surf, s.layout(10, 10))?;
            let first_index = s.u32_at(surf, s.layout(12, 12))?;
            let surf_index_count = (surf_tri_count as u32).saturating_mul(3) as u16;

            // Bounds checking: surface ranges must be within total geometry
            let fv = first_vertex as usize;
            let vc = surf_vertex_count as usize;
            let fi = first_index as usize;
            let ic = surf_index_count as usize;

            if fv > vertex_count || fv + vc > vertex_count {
                return Err(crate::ZoneError::Truncated {
                    at: 0,
                    needed: fv + vc,
                    len: vertex_count,
                });
            }
            if fi > index_count || fi + ic > index_count {
                return Err(crate::ZoneError::Truncated {
                    at: 0,
                    needed: fi + ic,
                    len: index_count,
                });
            }

            soups.push(TriangleSoup {
                material: 0, // fallback material
                lightmap: 0,
                first_vertex,
                vertex_count: surf_vertex_count,
                first_index,
                index_count: surf_index_count,
            });
        }
    }

    Ok((verts, indices, soups, materials))
}

/// Convert a parsed ClipMap into vcod collision geometry
pub fn clipmap_to_collision(
    s: &ZoneStream<'_>,
    clip: &ClipMapGeometry,
) -> Result<(Vec<Plane>, Vec<BrushSide>, Vec<Brush>, Vec<[f32; 3]>, Vec<u16>)> {
    // Sanity checks
    if clip.plane_count > MAX_PLANES {
        return Err(crate::ZoneError::Truncated { at: 0, needed: clip.plane_count, len: clip.plane_count });
    }
    if clip.brush_side_count > MAX_BRUSH_SIDES {
        return Err(crate::ZoneError::Truncated { at: 0, needed: clip.brush_side_count, len: clip.brush_side_count });
    }
    if clip.brush_count > MAX_BRUSHES {
        return Err(crate::ZoneError::Truncated { at: 0, needed: clip.brush_count, len: clip.brush_count });
    }
    if clip.vert_count > MAX_VERTS {
        return Err(crate::ZoneError::Truncated { at: 0, needed: clip.vert_count, len: clip.vert_count });
    }
    if clip.tri_count > MAX_TRIS {
        return Err(crate::ZoneError::Truncated { at: 0, needed: clip.tri_count, len: clip.tri_count });
    }

    // Read planes: 20 bytes each (normal: 12, dist: 4, type: 4)
    let mut planes = Vec::new();
    if let Some(pptr) = clip.planes {
        for i in 0..clip.plane_count {
            let base = pptr.at(i * 20);
            let normal = [
                s.f32_at(base, 0)?,
                s.f32_at(base, 4)?,
                s.f32_at(base, 8)?,
            ];
            let dist = s.f32_at(base, 12)?;
            planes.push(Plane { normal, dist });
        }
    }

    // Temporarily disable IW4 brush conversion - not yet properly decoded
// TODO: Native IW4 cbrush_t conversion still needs semantic decoding
let brush_sides = Vec::new();
let brushes = Vec::new();

// Read collision verts: 12 bytes each
    let mut collision_verts = Vec::new();
    if let Some(vptr) = clip.verts {
        for i in 0..clip.vert_count {
            let base = vptr.at(i * 12);
            collision_verts.push([
                s.f32_at(base, 0)?,
                s.f32_at(base, 4)?,
                s.f32_at(base, 8)?,
            ]);
        }
    }

    // Read collision indices: u16 each
    let mut collision_indices = Vec::new();
    if let Some(iptr) = clip.tri_indices {
        let tri_count = clip.tri_count;
        for i in 0..tri_count * 3 {
            collision_indices.push(s.u16_at(iptr, i * 2)?);
        }
    }

    Ok((planes, brush_sides, brushes, collision_verts, collision_indices))
}

/// Read the entity string from MapEnts
pub fn mapents_to_entities(
    s: &ZoneStream<'_>,
    map_ents: &MapEntsGeometry,
) -> Result<String> {
    if map_ents.entity_chars > MAX_ENTITY_CHARS {
        return Err(crate::ZoneError::Truncated {
            at: 0,
            needed: map_ents.entity_chars,
            len: map_ents.entity_chars,
        });
    }
    if let Some(eptr) = map_ents.entity_string {
        let mut result = String::new();
        for i in 0..map_ents.entity_chars {
            let byte = s.u8_at(eptr, i)?;
            result.push(byte as char);
        }
        Ok(result)
    } else {
        Ok(String::new())
    }
}

/// Build a complete vcod Bsp from parsed IW4 world data
pub fn build_bsp(
    s: &ZoneStream<'_>,
    gfx: &GfxWorldGeometry,
    clip: &ClipMapGeometry,
    map_ents: &MapEntsGeometry,
) -> Result<Bsp> {
    let (verts, indices, soups, mut materials) = gfxworld_to_draw_geometry(s, gfx)?;
    let (planes, brush_sides, brushes, collision_verts, collision_indices) =
        clipmap_to_collision(s, clip)?;
    let entities = mapents_to_entities(s, map_ents)?;

    // Ensure materials vector covers all brush material indices.
    // IW4 brush material indices refer to the IW4 material table which we
    // don't fully load; create fallback entries so indexing doesn't panic.
    // Set CONTENTS_SOLID so the collision system actually uses these brushes.
    let max_brush_mat = brushes.iter().map(|b| b.material as usize).max().unwrap_or(0);
    if max_brush_mat >= materials.len() {
        let fallback = materials.first().cloned().unwrap_or(Material {
            name: "iw4/fallback".to_string(),
            surface_flags: 0,
            content_flags: 0,
        });
        materials.resize(max_brush_mat + 1, fallback);
    }
    // Ensure all materials have CONTENTS_SOLID (0x1) so collision uses them
    for mat in &mut materials {
        mat.content_flags |= 0x1;
    }

    // Compute world bounds from REFERENCED vertices only.
    // IW4's vertexCount describes a shared vertex block, not a per-soup range.
    // Many soups share the same first_vertex/vertex_count but reference tiny
    // disjoint subsets. Using the declared range produces enormous false AABBs.
    let mut world_mins = [f32::INFINITY; 3];
    let mut world_maxs = [f32::NEG_INFINITY; 3];
    for soup in &soups {
        let fv = soup.first_vertex as usize;
        let fi = soup.first_index as usize;
        let ic = soup.index_count as usize;
        if fi + ic > indices.len() {
            continue;
        }
        for &local_idx in &indices[fi..fi + ic] {
            let global = fv + local_idx as usize;
            if global < verts.len() {
                let v = &verts[global];
                for a in 0..3 {
                    world_mins[a] = world_mins[a].min(v.pos[a]);
                    world_maxs[a] = world_maxs[a].max(v.pos[a]);
                }
            }
        }
    }
    // Small epsilon to avoid degenerate bounds.
    for a in 0..3 {
        if world_mins[a].is_finite() && world_maxs[a].is_finite() {
            world_mins[a] -= 1.0;
            world_maxs[a] += 1.0;
        }
    }

    // Build a single model covering all geometry with REAL bounds.
    let models = vec![vcod_common::bsp::Model {
        mins: world_mins,
        maxs: world_maxs,
        first_soup: 0,
        num_soups: soups.len() as u32,
        first_brush: 0,
        num_brushes: brushes.len() as u32,
    }];

    // Build a minimal but valid visibility graph:
    // - One cell covering the world bounds
    // - One AABB node with all soups (no children)
    // This lets visible_fallback mark all soups visible when the frustum
    // intersects the cell bounds.
    let cells = vec![Cell {
        mins: world_mins,
        maxs: world_maxs,
        first_aabb: 0,
        first_portal: 0,
        portal_count: 0,
        first_cull_index: 0,
        cull_count: 0,
        first_occluder: 0,
        occluder_count: 0,
    }];
    let aabb_nodes = vec![AabbNode {
        first_soup: 0,
        soup_count: soups.len() as u32,
        child_count: 0,
    }];

    // Build white lightmaps so geometry is visible without real MW2 lightmaps.
    // A black lightmap would multiply the texture to zero (invisible).
    let lightmaps: Vec<Vec<u8>> = (0..16)
        .map(|_| vec![255u8; 512 * 512 * 3])
        .collect();

    Ok(Bsp {
        materials,
        lightmaps,
        soups,
        verts,
        indices,
        entities,
        planes,
        brush_sides,
        brushes,
        models,
        cull_groups: Vec::new(),
        cull_indices: Vec::new(),
        portal_verts: Vec::new(),
        occluders: Vec::new(),
        occluder_plane_indices: Vec::new(),
        occluder_edges: Vec::new(),
        occluder_indices: Vec::new(),
        aabb_nodes,
        cells,
        portals: Vec::new(),
        nodes: Vec::new(),
        leafs: Vec::new(),
        pvs: None,
        terrain: Vec::new(),
        patches: Vec::new(),
        collision_verts,
        collision_indices,
    })
}