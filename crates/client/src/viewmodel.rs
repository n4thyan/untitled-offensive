//! The first-person weapon rig: loading the hands, gun and clips, and the
//! walk bob and mouse sway. No rendering or windowing types.

use glam::{Mat4, Vec2, Vec3, Vec4};
use std::collections::HashMap;
use vcod_common::pk3::Pk3Fs;
use vcod_common::{skeleton, weapon, xanim, xmodel};

pub struct ViewWeapon {
    pub skeleton: skeleton::Skeleton,
    pub pose: skeleton::PoseBuffer,
    pub state: weapon::WeaponState,
    pub def: weapon::WeaponDef,
    /// Missing entries fall back to Idle's clip.
    pub anims: HashMap<weapon::WeaponAnim, (xanim::XAnim, skeleton::AnimBinding)>,
}

/// Hands first so the gun draws over them and the shared skeleton takes the
/// hands' bones as its base. `None` if a model is missing (walk mode then has
/// no viewmodel); the inner `None` means the models loaded but the anims did not.
pub fn load_view_weapon(
    fs: &Pk3Fs,
    name: &str,
) -> Option<(Vec<xmodel::XModel>, Option<Box<ViewWeapon>>)> {
    load_view_weapon_with_hands(fs, name, None)
}

/// [`load_view_weapon`] with `hands_model` in place of the file's
/// `handModel`: retail draws the hands `ps.viewmodelIndex` names
/// (docs/research/cod11-gsc-object-model.md, "`setViewmodel` reaches
/// `ps.viewmodelIndex`"). The `xmodel/` prefix is optional.
pub fn load_view_weapon_with_hands(
    fs: &Pk3Fs,
    name: &str,
    hands_model: Option<&str>,
) -> Option<(Vec<xmodel::XModel>, Option<Box<ViewWeapon>>)> {
    let text = fs.read(&format!("weapons/mp/{name}"))?;
    let weapon = xmodel::parse_weapon(&String::from_utf8_lossy(&text));
    let hands = match hands_model {
        Some(h) => h.strip_prefix("xmodel/").unwrap_or(h),
        None => weapon.get("handModel")?,
    };
    let mut models = Vec::new();
    for (i, name) in [hands, weapon.get("gunModel")?].into_iter().enumerate() {
        match xmodel::load(fs, name) {
            Ok(mut m) => {
                if i == 0 {
                    xmodel::apply_viewhands_placeholder_override(&mut m);
                }
                models.push(m);
            }
            Err(e) => {
                log::warn!("viewmodel {name}: {e:#}");
                return None;
            }
        }
    }
    let animated = load_anims(fs, &weapon, &models).map(Box::new);
    Some((models, animated))
}

/// A clip that is unnamed or fails to load is skipped and its state plays
/// idle. Without idle there is no fallback, so the rig is dropped and the
/// viewmodel draws in bind pose.
pub fn load_anims(
    fs: &Pk3Fs,
    weapon: &HashMap<String, String>,
    models: &[xmodel::XModel],
) -> Option<ViewWeapon> {
    let [hands, gun] = models else {
        return None;
    };
    // Same order as set_viewmodel, so bone_sets[i] matches model i. The gun
    // hangs off `tag_weapon` whatever its root is called: the carbine's root
    // is `tag_origin` (docs/research/xmodel-v14-format.md, "Model space and
    // the view basis").
    let skeleton = skeleton::Skeleton::build_grafted(&[(hands, None), (gun, Some("tag_weapon"))]);

    let mut anims = HashMap::new();
    for which in weapon::WeaponAnim::ALL {
        let key = which.key();
        let Some(name) = weapon.get(key).map(|n| n.trim()).filter(|n| !n.is_empty()) else {
            log::debug!("weapon: no {key}, that state will play idle");
            continue;
        };
        match xanim::load(fs, name) {
            Ok(anim) => {
                let binding = skeleton.bind(&anim);
                anims.insert(which, (anim, binding));
            }
            Err(e) => log::warn!("xanim {name} ({key}): {e:#}"),
        }
    }
    if !anims.contains_key(&weapon::WeaponAnim::Idle) {
        log::warn!("no idle anim loaded; drawing the viewmodel statically");
        return None;
    }

    // Seeded with idle: a first clip that keys only `tag_torso` (a sight
    // already rising at the spawn) would leave the arms at the zeroed bind.
    let mut pose = skeleton::PoseBuffer::new(&skeleton);
    let (idle, binding) = &anims[&weapon::WeaponAnim::Idle];
    pose.apply(idle, binding, 0.0);
    let def = weapon::WeaponDef::from_map(weapon);
    Some(ViewWeapon {
        pose,
        skeleton,
        state: weapon::WeaponState::new(def.clone()),
        def,
        anims,
    })
}

impl ViewWeapon {
    /// Poses the sight layer: `AdsUp` at `frac` while `raising`, else
    /// `AdsDown` at `1 - frac`. Retail keeps one of the two weighted at all
    /// times, so the hip position is `AdsDown`'s last frame, not the bind
    /// pose (docs/research/xanim-v14-format.md, "The sight layer"). Call it
    /// before the main clip, which then overrides whatever it keys.
    pub fn pose_sight(&mut self, frac: f32, raising: bool) {
        let (which, t) = if raising {
            (weapon::WeaponAnim::AdsUp, frac)
        } else {
            (weapon::WeaponAnim::AdsDown, 1.0 - frac)
        };
        if let Some((clip, binding)) = self.anims.get(&which) {
            let t = t.clamp(0.0, 1.0) * clip.duration();
            self.pose.apply(clip, binding, clip.frame_pos(t, false));
        }
    }
}

/// Model space (X forward, Y left, Z up, tag_view at the eye) to view space
/// (X right, Y up, -Z forward). A rotation, so one-sided winding survives.
/// docs/research/xmodel-v14-format.md, "Model space and the view basis".
const BASIS: Mat4 = Mat4::from_cols(
    Vec4::new(0.0, 0.0, -1.0, 0.0), // model +X (forward) -> view -Z
    Vec4::new(-1.0, 0.0, 0.0, 0.0), // model +Y (left)    -> view -X
    Vec4::new(0.0, 1.0, 0.0, 0.0),  // model +Z (up)      -> view +Y
    Vec4::W,
);
/// Residual view-space offset (right, up, forward is -Z). Zero: the idle anim
/// already holds the rifle at the hip.
const OFFSET_POS: Vec3 = Vec3::ZERO;
/// Residual yaw about view up, positive swings the muzzle left.
const OFFSET_YAW_DEG: f32 = 0.0;
const BOB_CYCLE_HZ: f32 = 1.7; // full cycle at run speed
const BOB_LATERAL: f32 = 0.35; // view-space units at full speed
const BOB_VERTICAL: f32 = 0.25;
const BOB_ROLL_DEG: f32 = 0.6;
const BOB_ATTACK: f32 = 8.0; // 1/s, amplitude lerp rate
const SWAY_SCALE: f32 = 0.0015; // units per mouse count
const SWAY_MAX: f32 = 0.6; // clamp, view-space units
const SWAY_DECAY: f32 = 6.0; // 1/s exponential return
const SPRINT_LOWER: f32 = 2.0;
const SPRINT_SIDE: f32 = 0.45;
const SPRINT_ROLL_DEG: f32 = 6.0;
const SPRINT_YAW_DEG: f32 = 2.0;
const SPRINT_BOB: f32 = 0.12;
const SPRINT_BLEND_IN: f32 = 10.0;
const SPRINT_BLEND_OUT: f32 = 14.0;
const SPRINT_BOB_HZ: f32 = 1.35;

#[derive(Clone, Copy, Debug)]
pub struct SprintPoseInput {
    pub requested: bool,
    pub moving: bool,
    pub on_ground: bool,
    pub ads_frac: f32,
    pub weapon_anim: weapon::WeaponAnim,
}

#[derive(Clone, Copy, Debug)]
pub struct SprintPoseSample {
    pub blend: f32,
    pub offset: Vec3,
    pub roll_deg: f32,
    pub bob: Vec2,
}

#[derive(Default)]
pub struct SprintPose {
    blend: f32,
    phase: f32,
}

impl SprintPose {
    pub fn update(&mut self, dt: f32, input: SprintPoseInput) {
        let idle = matches!(
            input.weapon_anim,
            weapon::WeaponAnim::Idle | weapon::WeaponAnim::EmptyIdle
        );
        let active =
            input.requested && input.moving && input.on_ground && input.ads_frac <= 0.001 && idle;
        let target = if active { 1.0 } else { 0.0 };
        let rate = if active {
            SPRINT_BLEND_IN
        } else {
            SPRINT_BLEND_OUT
        };
        self.blend += (target - self.blend) * (1.0 - (-rate * dt.max(0.0)).exp());
        if (self.blend - target).abs() < 0.0001 {
            self.blend = target;
        }
        if active {
            self.phase = (self.phase + dt.max(0.0) * std::f32::consts::TAU * SPRINT_BOB_HZ)
                % std::f32::consts::TAU;
        }
    }

    pub fn sample(&self) -> SprintPoseSample {
        let bob = Vec2::new(self.phase.sin(), (self.phase * 2.0).sin()) * (SPRINT_BOB * self.blend);
        SprintPoseSample {
            blend: self.blend,
            offset: Vec3::new(SPRINT_SIDE * self.blend, -SPRINT_LOWER * self.blend, 0.0),
            roll_deg: SPRINT_ROLL_DEG * self.blend,
            bob,
        }
    }

    fn transform(&self) -> Mat4 {
        let p = self.sample();
        Mat4::from_translation(p.offset + Vec3::new(p.bob.x, p.bob.y, 0.0))
            * Mat4::from_rotation_z(p.roll_deg.to_radians())
            * Mat4::from_rotation_y((SPRINT_YAW_DEG * p.blend).to_radians())
    }
}

/// Walk bob cycle and mouse sway spring; `transform` is the view-space model
/// matrix.
pub struct ViewmodelMotion {
    phase: f32,
    amp: f32,
    sway: Vec2,
    sprint: SprintPose,
}

impl ViewmodelMotion {
    pub fn new() -> Self {
        Self {
            phase: 0.0,
            amp: 0.0,
            sway: Vec2::ZERO,
            sprint: SprintPose::default(),
        }
    }

    /// `ground_speed` is horizontal speed on the ground, else 0. Mouse deltas
    /// are raw counts. `damp` scales bob and sway, 1 full, 0 frozen; ADS lerps
    /// it toward the weapon's `adsViewBobMult`.
    pub fn update(
        &mut self,
        dt: f32,
        ground_speed: f32,
        on_ground: bool,
        mouse_dx: f32,
        mouse_dy: f32,
        damp: f32,
    ) {
        let speed_frac = (ground_speed / 190.0).clamp(0.0, 1.0);

        if on_ground && ground_speed > 1.0 {
            self.phase += dt * std::f32::consts::TAU * BOB_CYCLE_HZ * speed_frac;
            self.phase %= std::f32::consts::TAU;
        }

        let target = if on_ground { speed_frac * damp } else { 0.0 };
        self.amp += (target - self.amp) * (BOB_ATTACK * dt).min(1.0);

        self.sway += Vec2::new(mouse_dx, mouse_dy) * SWAY_SCALE * damp;
        self.sway = self.sway.clamp_length_max(SWAY_MAX);
        self.sway *= (-SWAY_DECAY * dt).exp();
    }

    pub fn update_sprint(&mut self, dt: f32, input: SprintPoseInput) {
        self.sprint.update(dt, input);
    }

    pub fn transform(&self) -> Mat4 {
        let bob = Vec3::new(
            self.phase.sin() * BOB_LATERAL * self.amp,
            -(self.phase * 2.0).sin().abs() * BOB_VERTICAL * self.amp,
            0.0,
        );
        let roll = self.phase.sin() * BOB_ROLL_DEG.to_radians() * self.amp;
        // The gun lags the turn: mouse right drifts it left, mouse down up.
        let sway_offset = Vec3::new(-self.sway.x, self.sway.y, 0.0);

        self.sprint.transform()
            * Mat4::from_translation(OFFSET_POS + bob + sway_offset)
            * Mat4::from_rotation_y(OFFSET_YAW_DEG.to_radians())
            * Mat4::from_rotation_z(roll)
            * BASIS
    }
}

impl Default for ViewmodelMotion {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drives the real kar98k through the redraw loop's calls without a window.
    /// Reaches idle, fire, rechamber, ADS up and ADS fire; not LastShot,
    /// AdsDown or Reloading.
    #[test]
    fn real_kar98k_animates_through_a_fire_and_ads_cycle() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let (models, view_weapon) = load_view_weapon(&fs, "kar98k_mp").expect("kar98k viewmodel");
        assert_eq!(models.len(), 2);
        let mut w = view_weapon.expect("kar98k anim rig");
        assert!(
            w.anims.contains_key(&weapon::WeaponAnim::Idle),
            "the rig only exists when idle loaded"
        );

        // fire every 40th frame so the bolt cycle completes; ADS for the second half
        let mut poses = Vec::new();
        for step in 0..240 {
            let out = w.state.update(
                1.0 / 60.0,
                weapon::WeaponInput {
                    fire: step > 60 && step % 40 == 0,
                    fire_held: false,
                    ads: step > 120,
                    reload: false,
                },
            );
            let (anim, binding) = w
                .anims
                .get(&out.anim)
                .or_else(|| w.anims.get(&weapon::WeaponAnim::Idle))
                .unwrap_or_else(|| panic!("{:?} has no clip and no idle fallback", out.anim));
            let frame = anim.frame_pos(out.anim_time, out.looping);
            assert!(
                frame.is_finite() && frame >= 0.0 && frame <= (anim.frame_count - 1) as f32,
                "{:?} frame {frame} out of range",
                out.anim
            );
            w.pose.apply(anim, binding, frame);
            for (i, model) in models.iter().enumerate() {
                let mats = w.pose.skin_matrices(&w.skeleton, i);
                assert_eq!(mats.len(), model.bones.len(), "model {i} bone count");
                assert!(mats.iter().all(|m| m.is_finite()), "model {i} step {step}");
            }
            poses.push(w.pose.skin_matrices(&w.skeleton, 1));
        }
        // a static rig would be a silent failure
        assert!(
            poses.iter().any(|p| p != &poses[0]),
            "the gun never moved across 240 frames"
        );
    }

    /// Model-space `tag_flash` (X forward, Y left, Z up) after the sight layer
    /// at `frac` and the idle clip on top, as a frame at rest draws it.
    fn rest_flash(w: &mut ViewWeapon, frac: f32) -> glam::Vec3 {
        w.pose_sight(frac, frac > 0.0);
        let (idle, binding) = &w.anims[&weapon::WeaponAnim::Idle];
        w.pose.apply(idle, binding, 0.0);
        let bi = w.skeleton.bone_index("tag_flash").expect("tag_flash");
        w.pose.bone_world(&w.skeleton, bi).0
    }

    /// At the hip the muzzle sits right of and below the eye; with the sight
    /// up it is on the view axis. The carbine's gun root is `tag_origin`, so
    /// it moves at all only through the `tag_weapon` graft.
    #[test]
    fn hip_holds_the_gun_low_right_and_the_sight_centres_it() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        for name in ["kar98k_mp", "m1carbine_mp"] {
            let (_, rig) = load_view_weapon(&fs, name).expect("viewmodel");
            let mut w = rig.expect("anim rig");
            let hip = rest_flash(&mut w, 0.0);
            assert!(hip.y < -2.0 && hip.z < -2.0, "{name} hip {hip}");
            let sight = rest_flash(&mut w, 1.0);
            assert!(
                sight.y.abs() < 0.5 && sight.z.abs() < 1.0,
                "{name} sight {sight}"
            );
        }
    }

    /// A sight already rising on the rig's first frame keys only `tag_torso`;
    /// the arms still hold the idle pose instead of the zeroed bind.
    #[test]
    fn a_fresh_rig_holds_the_idle_arms() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let (_, rig) = load_view_weapon(&fs, "m1carbine_mp").expect("carbine viewmodel");
        let mut w = rig.expect("anim rig");
        w.pose_sight(0.5, true);
        let at = |w: &ViewWeapon, bone: &str| {
            let bi = w.skeleton.bone_index(bone).expect(bone);
            w.pose.bone_world(&w.skeleton, bi).0
        };
        let reach = at(&w, "bip01 l hand").distance(at(&w, "tag_torso"));
        assert!(
            reach > 5.0,
            "the left hand collapsed onto tag_torso: {reach}"
        );
    }

    /// The nationality hands replace the file's `handModel`; the gun and the
    /// clips still come from the weapon file.
    #[test]
    fn hands_override_replaces_the_file_hand_model() {
        let Some(fs) = vcod_common::testing::game_fs() else {
            return;
        };
        let (own, _) = load_view_weapon(&fs, "m1carbine_mp").expect("carbine viewmodel");
        let (us, rig) =
            load_view_weapon_with_hands(&fs, "m1carbine_mp", Some("xmodel/viewmodel_hands_us"))
                .expect("carbine with the US hands");
        assert_eq!(us.len(), 2);
        // The hand files share one mesh and differ in their sleeve skins.
        assert_ne!(
            us[0].materials, own[0].materials,
            "hands must come from the override"
        );
        assert!(us[0].materials.iter().any(|m| m == "viewsleeves_new.tga"));
        assert_eq!(us[1].lod, own[1].lod, "the gun stays the file's");
        assert!(rig.is_some_and(|w| w.anims.contains_key(&weapon::WeaponAnim::Idle)));
    }

    #[test]
    fn bob_advances_only_when_moving_on_ground() {
        let mut m = ViewmodelMotion::new();
        let t0 = m.transform();
        m.update(0.016, 0.0, true, 0.0, 0.0, 1.0);
        assert!(m.transform().abs_diff_eq(t0, 1e-6), "idle must not bob");
        for _ in 0..30 {
            m.update(0.016, 190.0, true, 0.0, 0.0, 1.0);
        }
        assert!(!m.transform().abs_diff_eq(t0, 1e-4), "running must bob");
        for _ in 0..200 {
            m.update(0.016, 0.0, false, 0.0, 0.0, 1.0);
        }
        assert!(m.transform().abs_diff_eq(t0, 1e-3), "bob must decay in air");
    }

    #[test]
    fn sway_follows_mouse_and_decays() {
        let mut m = ViewmodelMotion::new();
        let t0 = m.transform();
        m.update(0.016, 0.0, true, 500.0, 0.0, 1.0);
        let swayed = m.transform();
        assert!(!swayed.abs_diff_eq(t0, 1e-5), "mouse motion must sway");
        for _ in 0..300 {
            m.update(0.016, 0.0, true, 0.0, 0.0, 1.0);
        }
        assert!(
            m.transform().abs_diff_eq(t0, 1e-3),
            "sway must decay to rest"
        );
    }

    #[test]
    fn sway_is_clamped() {
        let mut m = ViewmodelMotion::new();
        for _ in 0..100 {
            m.update(0.016, 0.0, true, 10000.0, 0.0, 1.0);
        }
        let a = m.transform();
        for _ in 0..100 {
            m.update(0.016, 0.0, true, 10000.0, 0.0, 1.0);
        }
        assert!(m.transform().abs_diff_eq(a, 1e-3), "sway must saturate");
    }

    /// Screenshot-verified sign.
    #[test]
    fn sway_direction_is_pinned() {
        let mut right = ViewmodelMotion::new();
        right.update(0.016, 0.0, true, 500.0, 0.0, 1.0);
        assert!(
            right.transform().w_axis.x < OFFSET_POS.x,
            "mouse right must sway the gun toward view -x"
        );

        let mut down = ViewmodelMotion::new();
        down.update(0.016, 0.0, true, 0.0, 500.0, 1.0);
        assert!(
            down.transform().w_axis.y > OFFSET_POS.y,
            "mouse down must sway the gun toward view +y"
        );
    }

    #[test]
    fn sprint_pose_enters_smoothly_and_stays_restrained() {
        let mut pose = SprintPose::default();
        let active = SprintPoseInput {
            requested: true,
            moving: true,
            on_ground: true,
            ads_frac: 0.0,
            weapon_anim: weapon::WeaponAnim::Idle,
        };

        pose.update(0.016, active);
        let first = pose.sample();
        assert!(
            first.blend > 0.0 && first.blend < 1.0,
            "first frame must interpolate"
        );
        assert!(first.offset.y < 0.0, "the weapon lowers in view space");
        assert!(
            first.roll_deg.abs() > 0.0,
            "the weapon rotates slightly aside"
        );

        for _ in 0..120 {
            pose.update(0.016, active);
        }
        let settled = pose.sample();
        assert!(settled.blend > 0.99);
        assert!(
            settled.offset.y >= -2.5,
            "lowering remains restrained: {settled:?}"
        );
        assert!(
            settled.roll_deg.abs() <= 8.0,
            "rotation remains restrained: {settled:?}"
        );
        assert!(
            settled.bob.length() <= 0.2,
            "sprint bob remains restrained: {settled:?}"
        );
    }

    #[test]
    fn sprint_pose_recovers_after_every_weapon_interruption() {
        let active = SprintPoseInput {
            requested: true,
            moving: true,
            on_ground: true,
            ads_frac: 0.0,
            weapon_anim: weapon::WeaponAnim::Idle,
        };
        for interruption in [
            SprintPoseInput {
                ads_frac: 0.2,
                ..active
            },
            SprintPoseInput {
                weapon_anim: weapon::WeaponAnim::Fire,
                ..active
            },
            SprintPoseInput {
                weapon_anim: weapon::WeaponAnim::Reload,
                ..active
            },
            SprintPoseInput {
                weapon_anim: weapon::WeaponAnim::Melee,
                ..active
            },
            SprintPoseInput {
                requested: false,
                ..active
            },
        ] {
            let mut pose = SprintPose::default();
            for _ in 0..60 {
                pose.update(0.016, active);
            }
            let sprinted = pose.sample().blend;
            pose.update(0.016, interruption);
            let interrupted = pose.sample().blend;
            assert!(
                interrupted < sprinted && interrupted > 0.0,
                "interruption must recover smoothly"
            );
            for _ in 0..90 {
                pose.update(0.016, interruption);
            }
            assert!(
                pose.sample().blend < 0.001,
                "interruption must reach neutral"
            );
            pose.update(0.016, active);
            assert!(
                pose.sample().blend > 0.0,
                "held sprint resumes when the interruption ends"
            );
        }
    }

    #[test]
    fn damping_freezes_bob_and_sway() {
        let mut m = ViewmodelMotion::new();
        let rest = m.transform();
        for _ in 0..200 {
            m.update(0.016, 190.0, true, 500.0, 500.0, 0.0);
        }
        assert!(
            m.transform().abs_diff_eq(rest, 1e-6),
            "damp 0 must leave the transform at rest, got {:?}",
            m.transform()
        );

        // the same inputs at full damping must move it
        let mut live = ViewmodelMotion::new();
        for _ in 0..200 {
            live.update(0.016, 190.0, true, 500.0, 500.0, 1.0);
        }
        assert!(!live.transform().abs_diff_eq(rest, 1e-4));
    }
}
