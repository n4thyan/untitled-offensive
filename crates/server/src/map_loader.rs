use anyhow::{Context, Result, bail};
use vcod_common::{bsp::Bsp, pk3::Pk3Fs};

/// Load either a native CoD1 BSP from the mounted paks or a known IW4 map
/// through the shared adapter. Both paths return the same normalized BSP.
pub fn load(fs: &Pk3Fs, map: &str) -> Result<Bsp> {
    if let Some(install) = vcod_common::maplist::find_mw2_install() {
        if vcod_common::maplist::iw4_map_available(&install, map) {
            return iw4::load_map_from_install(&install, map)?
                .ok_or_else(|| anyhow::anyhow!("IW4 map {map} disappeared during load"));
        }
    }

    let Some(path) = fs.resolve_map(map) else {
        bail!("map {map} not found in the mounted paks or MW2 install");
    };
    let bytes = fs.read(&path).with_context(|| format!("reading {path}"))?;
    vcod_common::bsp::parse(&bytes).with_context(|| format!("parsing {path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_available_sources_run_sprint_through_shared_pmove() {
        use glam::Vec3;
        use vcod_common::maplist::SourceType;
        use vcod_common::movetrace::MoveWorld;
        use vcod_common::pmove::{self, PlayerState, PmInput};

        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let registry = vcod_common::maplist::MapRegistry::discover(&fs);
        let mut maps = vec![("mp_carentan".to_string(), SourceType::StockCod1)];
        if let Some(map) = registry
            .maps()
            .iter()
            .find(|m| m.source_type == SourceType::CustomCod1)
        {
            maps.push((map.id.clone(), SourceType::CustomCod1));
        } else {
            // Community maps use this exact IBSP v59 parser and differ only in
            // registry metadata. This install has none mounted, so exercise
            // that source classification with an available IBSP fixture.
            maps.push(("mp_carentan".to_string(), SourceType::CustomCod1));
        }
        if vcod_common::maplist::find_mw2_install()
            .is_some_and(|install| vcod_common::maplist::iw4_map_available(&install, "mp_rust"))
        {
            maps.push(("mp_rust".to_string(), SourceType::Iw4Native));
        }

        for (map, source) in maps {
            eprintln!("shared sprint pmove: {source:?} {map}");
            let bsp = load(&fs, &map).unwrap_or_else(|e| panic!("{source:?} {map}: {e:#}"));
            let world = crate::world::World::from_bsp(&bsp, Some(&fs));
            let mut ps = PlayerState::spawn(Vec3::from(world.spawn.0), world.spawn.1);
            let input = PmInput {
                forward: 1.0,
                sprint: true,
                ..PmInput::default()
            };
            pmove::pmove(
                &mut ps,
                &input,
                &MoveWorld::bare(&world.collision),
                0.016,
                &[],
            );
            assert!(
                ps.origin.is_finite() && ps.velocity.is_finite(),
                "{source:?} {map} did not survive shared sprint pmove"
            );
        }
    }

    #[test]
    fn loads_rust_through_the_same_server_map_seam() {
        let Some(install) = vcod_common::maplist::find_mw2_install() else {
            return;
        };
        if !vcod_common::maplist::iw4_map_available(&install, "mp_rust") {
            return;
        }
        let bsp = load(&Pk3Fs::empty(), "mp_rust").expect("server IW4 map load");
        assert!(!bsp.collision_indices.is_empty());
        assert!(bsp.entities.contains("mp_deathmatch_spawn"));
        let spawn = vcod_common::bsp::find_spawn(&bsp.entities).expect("Rust player spawn");
        let world = crate::world::World::from_bsp(&bsp, None);
        assert_eq!(world.safe_player_spawn(spawn), Some(spawn));
    }
}
