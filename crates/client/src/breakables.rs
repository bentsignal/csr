//! Presentation consumes health state after fixed-tick authority/prediction.
//! Keep the authored brush meshes/materials/lightmaps alive for round reset.
use crate::{LoadedLevel, player::{LocalPlayer, SimWorld}, visibility::MapClusters};
use bevy::prelude::*;

#[derive(Component)]
pub(crate) struct BreakableMesh(pub usize);

pub(crate) fn sync(level: Res<LoadedLevel>, world: Res<SimWorld>, local: Res<LocalPlayer>,
    mut cached_pvs: Local<Option<(i16, bool, Option<Vec<u8>>)>>,
    mut batches: Query<(&BreakableMesh, Option<&MapClusters>, &mut Visibility)>) {
    if world.0.breakables.is_empty() { return; }
    let bsp = &level.0.bsp;
    let cluster = bsp.leafs.get(bsp.point_leaf(local.render_eye)).map_or(-1, |leaf| leaf.cluster);
    let no_pvs = std::env::var_os("CSR_NO_PVS").is_some();
    if level.is_added() || cached_pvs.as_ref().is_none_or(|(old, bypass, _)| *old != cluster || *bypass != no_pvs) {
        *cached_pvs = Some((cluster, no_pvs, if no_pvs { None } else { bsp.cluster_pvs(cluster) }));
    }
    let row = cached_pvs.as_ref().and_then(|(_, _, row)| row.as_deref());
    for (leaf, clusters, mut visibility) in &mut batches {
        let intact = world.0.breakables.get(leaf.0).is_some_and(|state| !state.broken());
        let visible = intact && clusters.is_none_or(|clusters| clusters.visible(row));
        let next = if visible { Visibility::Inherited } else { Visibility::Hidden };
        if *visibility != next { *visibility = next; }
    }
}
