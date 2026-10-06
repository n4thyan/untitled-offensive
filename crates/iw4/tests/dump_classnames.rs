//! Dump classnames from mp_rust.ff MapEnts to understand spawn entity format

#[test]
fn dump_rust_classnames() {
    let path = r"C:\Program Files (x86)\Steam\steamapps\common\Call of Duty Modern Warfare 2\zone\english\mp_rust.ff";
    if !std::path::Path::new(path).exists() {
        eprintln!("SKIP: mp_rust.ff not found");
        return;
    }
    let zone = iw4::open_zone(std::path::Path::new(path)).expect("open");
    let header = zone.header().expect("header");
    let mut memory = iw4::ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&zone.bytes).expect("stream");

    struct FullCap {
        map_ents: Option<iw4::MapEntsGeometry>,
    }
    impl iw4::AssetSink for FullCap {
        fn set_script_strings(&mut self, _: iw4::ScriptStrings) {}
        fn begin_assets(&mut self, _: usize) {}
        fn load_asset(
            &mut self,
            s: &mut iw4::ZoneStream<'_>,
            _: usize,
            ty: iw4::AssetType,
            slot: iw4::Ptr,
        ) -> iw4::Result<()> {
            iw4::load_asset_at(s, ty, slot)?;
            if let Some(m) = s.map_ents() {
                self.map_ents = Some(m);
            }
            Ok(())
        }
    }

    let mut full_cap = FullCap { map_ents: None };
    iw4::load_zone(&mut stream, &mut full_cap).expect("load");
    let map_ents = full_cap.map_ents.expect("no map_ents");

    let mut entities = String::new();
    if let Some(eptr) = map_ents.entity_string {
        for i in 0..map_ents.entity_chars {
            let byte = stream.u8_at(eptr, i).unwrap();
            entities.push(byte as char);
        }
    }

    use std::collections::BTreeSet;
    let mut classnames: BTreeSet<String> = BTreeSet::new();
    for line in entities.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("\"classname\"") {
            if let Some(cn) = trimmed
                .strip_prefix("\"classname\"")
                .map(|s| s.trim().trim_matches('"'))
            {
                classnames.insert(cn.to_string());
            }
        }
    }
    eprintln!("=== Classnames in mp_rust MapEnts ===");
    for cn in &classnames {
        eprintln!("  {}", cn);
    }
    eprintln!("Total: {}", classnames.len());
    eprintln!("Entity chars: {}", map_ents.entity_chars);

    // Also dump first 2000 chars of entity string for inspection
    eprintln!("=== First 2000 chars of entity string ===");
    eprintln!("{}", &entities[..entities.len().min(2000)]);
}
