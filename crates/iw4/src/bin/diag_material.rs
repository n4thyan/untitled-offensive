//! Diagnostic: trace GfxSurface -> Material -> texture table -> GfxImage name.
//!
//! Usage: cargo run -p iw4 --bin diag_material -- <path-to-mp_rust.ff>

use std::collections::HashMap;

use iw4::{
    AssetLinkSink, AssetSink, AssetType, ClipMapGeometry, GfxWorldGeometry, MapEntsGeometry, Ptr,
    ScriptStrings, ZoneMemory, ZonePtr, ZoneStream, load_zone, open_zone,
};

#[derive(Default)]
struct Capture {
    gfx_world: Option<GfxWorldGeometry>,
    clip_map: Option<ClipMapGeometry>,
    map_ents: Option<MapEntsGeometry>,
    /// material body ptr -> material name
    mat_by_body: HashMap<(u8, u32), String>,
    /// material name -> first image name
    first_image: HashMap<String, String>,
    image_formats: HashMap<String, (u32, u16, u16)>,
    mat_names: Vec<String>,
    image_names: Vec<String>,
}

fn key(p: Ptr) -> (u8, u32) {
    (p.block, p.offset)
}

impl Capture {
    fn record_material(&mut self, s: &ZoneStream<'_>) {
        let Some(m) = s.latest_material() else { return };
        let name = m
            .name
            .and_then(|p| s.cstr(p).ok())
            .unwrap_or("<noname>")
            .to_string();
        if let Some(h) = m.header {
            self.mat_by_body.insert(key(h), name.clone());
        }
        self.mat_names.push(name.clone());

        // First texture slot's image name.
        if let Some(arr) = m.textures {
            if let Ok(ZonePtr::Offset(img_slot)) = s.ptr_at(arr, 8) {
                let body = s.resolve_alias(img_slot);
                if let Ok(ZonePtr::Offset(n)) = s.ptr_at(body, s.layout(28, 32)) {
                    if let Ok(img_name) = s.cstr(s.resolve_alias(n)) {
                        self.first_image
                            .insert(name.clone(), img_name.to_string());
                    }
                }
            }
        }
    }

    fn record_image(&mut self, s: &ZoneStream<'_>) {
        let Some(i) = s.latest_image() else { return };
        let name = i
            .name
            .and_then(|p| s.cstr(p).ok())
            .unwrap_or("<noname>")
            .to_string();
        self.image_formats
            .insert(name.clone(), (i.format, i.width, i.height));
        self.image_names.push(name);
    }
}

impl AssetLinkSink for Capture {
    fn loaded(
        &mut self,
        s: &ZoneStream<'_>,
        ty: AssetType,
        _slot: Ptr,
        _insert_slot: Option<Ptr>,
    ) -> Result<(), iw4::ZoneError> {
        match ty {
            AssetType::Material => self.record_material(s),
            AssetType::Image => self.record_image(s),
            _ => {}
        }
        Ok(())
    }

    fn alias(&mut self, _ty: AssetType, _slot: Ptr, _target: Ptr) -> Result<(), iw4::ZoneError> {
        Ok(())
    }
}

impl AssetSink for Capture {
    fn set_script_strings(&mut self, _strings: ScriptStrings) {}
    fn begin_assets(&mut self, _count: usize) {}
    fn load_asset(
        &mut self,
        s: &mut ZoneStream<'_>,
        _index: usize,
        ty: AssetType,
        slot: Ptr,
    ) -> Result<(), iw4::ZoneError> {
        iw4::load_asset_at_observed(s, ty, slot, self)?;
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
    let path = args.get(1).cloned().unwrap_or_else(|| {
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
    println!("=== Material/Texture chain diagnostic ===\n");
    let zone = open_zone(path).expect("open zone");
    let header = zone.header().expect("header");
    let mut memory = ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&zone.bytes).expect("stream");

    let mut capture = Capture::default();
    load_zone(&mut stream, &mut capture).expect("load zone");

    println!("wire format: {:?}  pointer_bytes: {}", stream.wire_format(), stream.pointer_bytes());
    println!("materials captured: {}", capture.mat_names.len());
    println!("images captured:    {}", capture.image_names.len());
    println!(
        "materials with a first-image name: {}",
        capture.first_image.len()
    );

    let gfx = capture.gfx_world.as_ref().expect("gfx world");
    let stride = stream.layout(24, 32);
    let surfaces = gfx.surfaces.expect("surfaces");

    // Resolve each surface's material pointer to a body, then to a name.
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut unresolved = 0usize;
    let mut ptr_kinds: HashMap<&str, usize> = HashMap::new();
    for i in 0..gfx.surface_count {
        let surf = surfaces.at(i * stride);
        let Ok(zp) = stream.ptr_at(surf, 16) else {
            unresolved += 1;
            continue;
        };
        let kind = match zp {
            ZonePtr::Null => "null",
            ZonePtr::Offset(_) => "offset",
            ZonePtr::Following => "following",
            ZonePtr::Insert => "insert",
        };
        *ptr_kinds.entry(kind).or_default() += 1;
        let body = match zp {
            ZonePtr::Offset(p) => Some(stream.resolve_alias(p)),
            _ => None,
        };
        match body.and_then(|b| capture.mat_by_body.get(&key(b))) {
            Some(n) => *counts.entry(n.clone()).or_default() += 1,
            None => unresolved += 1,
        }
    }

    println!("\nsurface material pointer kinds: {ptr_kinds:?}");
    println!(
        "surfaces resolved to a named material: {}",
        gfx.surface_count - unresolved
    );

    // Debug: sample keys from both sides.
    let mut sample_surf: Vec<(u8, u32)> = Vec::new();
    for i in 0..3 {
        let surf = surfaces.at(i * stride);
        if let Ok(ZonePtr::Offset(p)) = stream.ptr_at(surf, 16) {
            sample_surf.push(key(stream.resolve_alias(p)));
        }
    }
    let mut sample_mat: Vec<(u8, u32)> = capture.mat_by_body.keys().copied().take(3).collect();
    sample_mat.sort();
    println!("sample surface body keys: {sample_surf:?}");
    println!("sample material body keys: {sample_mat:?}");

    // DECISIVE TEST: read the material struct directly at the surface's own
    // pointer. If the runtime block holds real materials, the name is right there.
    println!("\n--- direct read at surface material pointers ---");
    let mut distinct: Vec<(u8, u32)> = Vec::new();
    for i in 0..gfx.surface_count {
        let surf = surfaces.at(i * stride);
        if let Ok(ZonePtr::Offset(p)) = stream.ptr_at(surf, 16) {
            if !distinct.contains(&key(p)) {
                distinct.push(key(p));
            }
        }
    }
    println!("distinct surface material pointers: {}", distinct.len());

    // Raw hex of the first 4 surface records, so the real layout is visible.
    println!("\n--- raw surface records (stride {stride}) ---");
    for i in 0..4 {
        let surf = surfaces.at(i * stride);
        let bytes = stream.slice_at(surf, 0, stride).unwrap_or(&[]);
        let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let u32s: Vec<u32> = (0..stride / 4)
            .map(|w| stream.u32_at(surf, w * 4).unwrap_or(0))
            .collect();
        println!("  surf {i} @block{}off{}: {}", surf.block, surf.offset, hex.join(" "));
        println!("      u32: {u32s:?}");
    }
    for (n, k) in distinct.iter().take(12).enumerate() {
        let p = Ptr {
            block: k.0,
            offset: k.1,
        };
        let name = match stream.ptr_at(p, 0) {
            Ok(ZonePtr::Offset(np)) => stream
                .cstr(stream.resolve_alias(np))
                .map(|c| c.to_string())
                .unwrap_or_else(|e| format!("<cstr err {e:?}>")),
            Ok(other) => format!("<name ptr {other:?}>"),
            Err(e) => format!("<ptr err {e:?}>"),
        };
        let tex_count = stream.u8_at(p, stream.layout(72, 80)).unwrap_or(0);
        println!("  [{n}] block{} off{}  name={name}  texture_count={tex_count}", k.0, k.1);
    }

    let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    // STRIDE SCAN: score each candidate stride by whether the material pointer
    // field yields distinct, plausible material names across surfaces.
    println!("\n--- stride scan ---");
    for cand in [20usize, 24, 28, 32, 36, 40, 44] {
        for field in [12usize, 16, 20] {
            let mut names: Vec<String> = Vec::new();
            let mut ok = 0usize;
            for i in 0..gfx.surface_count.min(400) {
                let surf = surfaces.at(i * cand);
                let Ok(ZonePtr::Offset(p)) = stream.ptr_at(surf, field) else {
                    continue;
                };
                // pointer must land in a block, and the name must be printable
                let Ok(ZonePtr::Offset(np)) = stream.ptr_at(p, 0) else {
                    continue;
                };
                let Ok(nm) = stream.cstr(stream.resolve_alias(np)) else {
                    continue;
                };
                if nm.len() >= 3
                    && nm.len() < 80
                    && nm.chars().all(|c| c.is_ascii_graphic() || c == ' ')
                {
                    ok += 1;
                    if !names.contains(&nm.to_string()) {
                        names.push(nm.to_string());
                    }
                }
            }
            if ok > 50 {
                println!(
                    "  stride={cand} field={field}: {ok}/400 resolved, {} distinct; e.g. {:?}",
                    names.len(),
                    names.iter().take(3).collect::<Vec<_>>()
                );
            }
        }
    }

    println!("\n--- top 30 materials by surface count ---");
    for (name, n) in ranked.iter().take(30) {
        let img = capture
            .first_image
            .get(name)
            .map(|s| s.as_str())
            .unwrap_or("<no image>");
        let fmt = capture
            .image_formats
            .get(img)
            .map(|(f, w, h)| format!("fmt={f} {w}x{h}"))
            .unwrap_or_default();
        println!("  {n:5}  {name}  ->  images/{img}.iwi  {fmt}");
    }

    println!("\n=== done ===");
}
