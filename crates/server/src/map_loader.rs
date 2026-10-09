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
    let bytes = fs
        .read(&path)
        .with_context(|| format!("reading {path}"))?;
    vcod_common::bsp::parse(&bytes).with_context(|| format!("parsing {path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
