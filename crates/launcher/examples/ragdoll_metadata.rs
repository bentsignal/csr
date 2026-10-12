//! Read-only installed-data invariants. No graphics, physics step or asset export.
use mdl::phy::ragdoll::Ragdoll;
use std::{collections::HashSet, error::Error};

fn main() -> Result<(), Box<dyn Error>> {
    let (_, fs) = level::mount_game(None)?;
    for path in [
        "models/player/custom_player/legacy/ctm_sas",
        "models/player/custom_player/legacy/tm_phoenix",
    ] {
        let bytes = fs
            .read(&format!("{path}.mdl"))
            .ok_or("installed MDL is missing")?;
        let model = mdl::Model::load_header(&bytes)?;
        let checksum = u32::from_le_bytes(bytes[8..12].try_into()?);
        let phy = fs
            .read(&format!("{path}.phy"))
            .ok_or("installed PHY is missing")?;
        let ragdoll = Ragdoll::parse(&phy)?;
        let binding = ragdoll.bind(checksum, &model.bones)?;
        assert_eq!(ragdoll.solids.len(), 16);
        assert_eq!(ragdoll.constraints.len(), 15);
        assert_eq!(binding.solid_bones.len(), 16);
        assert_eq!(ragdoll.collision_pairs.len(), 202);
        assert_eq!(ragdoll.joint_merges.len(), 2);
        assert_eq!(ragdoll.total_mass, 300.);
        let mass: f32 = ragdoll.solids.iter().map(|s| s.mass).sum();
        assert!((mass - ragdoll.total_mass).abs() < 0.001);
        let unique_pairs: HashSet<_> = ragdoll
            .collision_pairs
            .iter()
            .map(|(a, b)| if a.0 < b.0 { (*a, *b) } else { (*b, *a) })
            .collect();
        assert_eq!(unique_pairs.len(), 101);
        let static_phy = mdl::Phy::parse(&phy)?;
        let hulls: Vec<_> = ragdoll.solids.iter().flat_map(|s| &s.hulls).collect();
        assert_eq!(hulls.len(), 16);
        assert_eq!(hulls.len(), static_phy.hulls.len());
        for (a, b) in hulls.iter().zip(&static_phy.hulls) {
            assert_eq!(a.points, b.points);
            assert_eq!(a.triangles, b.triangles);
        }
        // Wrong-model binding fails rather than associating solids by ordinal.
        assert!(ragdoll.bind(checksum ^ 1, &model.bones).is_err());
        println!(
            "{path}: checksum={checksum} solids=16 constraints=15 hulls=16 bound=16 collision_pairs=202 failures: 0"
        );
    }
    Ok(())
}
