//! PVS rejection for static map batches, followed by Bevy's frustum culling.
use bevy::prelude::*;
use bevy::mesh::VertexAttributeValues;
#[derive(Component)]
pub struct MapClusters {
    clusters: Vec<usize>,
    bounds: Option<crate::shadow_diagnostic::Bounds>,
}

impl MapClusters {
    /// Include every leaf touching the full batch bounds, not just its centre.
    /// This deliberately overestimates visibility for large surfaces/props.
    pub fn mesh(bsp: &bsp::Bsp, mesh: &Mesh) -> Self {
        let Some(VertexAttributeValues::Float32x3(vertices)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return Self { clusters:Vec::new(),bounds:None }; };
        let (mut lo, mut hi) = (glam::Vec3::splat(f32::INFINITY), glam::Vec3::splat(f32::NEG_INFINITY));
        for p in vertices {
            let p = crate::coords::to_source(Vec3::from_array(*p));
            lo = lo.min(p); hi = hi.max(p);
        }
        lo -= glam::Vec3::splat(8.0); hi += glam::Vec3::splat(8.0);
        let mut clusters: Vec<_> = bsp.leafs.iter().filter(|leaf| leaf.cluster >= 0).filter(|leaf| {
            (0..3).all(|i| hi[i] >= leaf.mins[i] as f32 && lo[i] <= leaf.maxs[i] as f32)
        }).map(|leaf| leaf.cluster as usize).collect();
        clusters.sort_unstable(); clusters.dedup();
        Self { clusters,bounds:crate::shadow_diagnostic::Bounds::new(lo,hi) }
    }
    pub(crate) fn bounds(&self) -> Option<crate::shadow_diagnostic::Bounds> { self.bounds }
    pub(crate) fn visible(&self, row: Option<&[u8]>) -> bool {
        let Some(row) = row else { return true; };
        self.clusters.is_empty() || self.clusters.iter().any(|&c| row.get(c / 8).is_none_or(|b| b & (1 << (c % 8)) != 0))
    }
}

pub fn update(level: Res<crate::LoadedLevel>, local: Res<crate::player::LocalPlayer>,
    mut previous: Local<Option<i16>>, mut batches: Query<(&MapClusters, &mut Visibility)>) {
    let bsp = &level.0.bsp;
    let cluster = bsp.leafs.get(bsp.point_leaf(local.render_eye)).map_or(-1, |l| l.cluster);
    if !level.is_added() && *previous == Some(cluster) { return; }
    *previous = Some(cluster);
    let row = if std::env::var_os("CSR_NO_PVS").is_some() { None } else { bsp.cluster_pvs(cluster) };
    for (clusters, mut visibility) in &mut batches {
        visibility.set_if_neq(if clusters.visible(row.as_deref()) { Visibility::Inherited } else { Visibility::Hidden });
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn batches_keep_any_visible_leaf_and_fail_open() {
        let clusters=|clusters|MapClusters { clusters,bounds:None };
        assert!(clusters(vec![1,8]).visible(Some(&[0,1])));
        assert!(!clusters(vec![1,8]).visible(Some(&[1,0])));
        assert!(clusters(vec![]).visible(Some(&[0])));
        assert!(clusters(vec![99]).visible(Some(&[0])));
        assert!(clusters(vec![1]).visible(None));
    }
}

#[cfg(test)]
mod real_bevy_tests {
    use super::*;

    #[derive(Resource, Default)]
    struct VisibilityChanges(Vec<Entity>);

    fn record_visibility_changes(
        changed: Query<Entity, (With<MapClusters>, Changed<Visibility>)>,
        mut seen: ResMut<VisibilityChanges>,
    ) {
        seen.0 = changed.iter().collect();
    }

    fn fixture_level() -> crate::LoadedLevel {
        // A synthetic empty BSP loaded only through an in-memory VFS. No game
        // install, user configuration, window or renderer is involved.
        let mut bytes = vec![0; 8 + 64 * 16 + 4];
        bytes[..4].copy_from_slice(b"VBSP");
        bytes[4..8].copy_from_slice(&21i32.to_le_bytes());
        let mut fs = vfs::Vfs::new();
        fs.push_back(
            "synthetic",
            vfs::Mount::Memory(std::collections::BTreeMap::from([(
                "maps/pvs-fixture.bsp".into(),
                std::sync::Arc::from(bytes),
            )])),
        );
        let mut level = level::Level::load_from_vfs(
            content::ContentId::csgo(content::ContentKind::Map, "pvs-fixture"),
            "pvs-fixture",
            "no-game-install".into(),
            fs,
        )
        .unwrap();
        level.bsp.planes.push(bsp::Plane {
            normal: glam::Vec3::X,
            dist: 0.0,
            kind: 0,
        });
        level.bsp.nodes.push(bsp::Node {
            plane: 0,
            children: [-1, -2],
            mins: [-16; 3],
            maxs: [16; 3],
            first_face: 0,
            num_faces: 0,
        });
        level.bsp.leafs = (0..2)
            .map(|cluster| bsp::Leaf {
                contents: 0,
                cluster,
                area: 0,
                mins: [-16; 3],
                maxs: [16; 3],
                first_leaf_face: 0,
                num_leaf_faces: 0,
                first_leaf_brush: 0,
                num_leaf_brushes: 0,
            })
            .collect();
        // Two rows: cluster zero sees only zero; cluster one sees only one.
        level.bsp.visibility.extend(2i32.to_le_bytes());
        for offset in [20i32, 21] {
            level.bsp.visibility.extend(offset.to_le_bytes());
            level.bsp.visibility.extend((-1i32).to_le_bytes());
        }
        level.bsp.visibility.extend([1, 2]);
        crate::LoadedLevel(level)
    }

    fn fixture_app() -> App {
        assert!(
            std::env::var_os("CSR_NO_PVS").is_none(),
            "run PVS fixtures without CSR_NO_PVS"
        );
        bevy::tasks::ComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let mut app = App::new();
        app.add_plugins(bevy::camera::visibility::VisibilityPlugin)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>()
            .init_resource::<VisibilityChanges>()
            .insert_resource(fixture_level())
            .insert_resource(crate::player::LocalPlayer::new(1, glam::Vec3::X, 0.0))
            .add_systems(Update, update)
            .add_systems(Last, record_visibility_changes);
        app
    }

    fn spawn_batch(app: &mut App, clusters: Vec<usize>, visibility: Visibility) -> Entity {
        app.world_mut()
            .spawn((
                MapClusters {
                    clusters,
                    bounds: None,
                },
                visibility,
            ))
            .id()
    }

    fn expect_changes(app: &App, expected: &[Entity]) {
        let seen = &app.world().resource::<VisibilityChanges>().0;
        assert_eq!(seen.len(), expected.len(), "changed batches: {seen:?}");
        assert!(
            expected.iter().all(|entity| seen.contains(entity)),
            "changed batches: {seen:?}"
        );
    }

    fn expect_visibility(app: &App, entity: Entity, visible: bool) {
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            }
        );
        // Exercise Bevy's real PostUpdate propagation as well as our enum write.
        assert_eq!(
            app.world()
                .get::<InheritedVisibility>(entity)
                .unwrap()
                .get(),
            visible
        );
    }

    #[test]
    fn real_bevy_visibility_settles_and_cluster_changes_propagate() {
        let mut app = fixture_app();
        let a = spawn_batch(&mut app, vec![0], Visibility::Inherited);
        let b = spawn_batch(&mut app, vec![1], Visibility::Inherited);
        let both = spawn_batch(&mut app, vec![0, 1], Visibility::Inherited);
        let empty = spawn_batch(&mut app, vec![], Visibility::Inherited);
        let unknown = spawn_batch(&mut app, vec![99], Visibility::Inherited);
        let child = app
            .world_mut()
            .spawn((Visibility::Inherited, ChildOf(b)))
            .id();
        app.update();
        expect_changes(&app, &[a, b, both, empty, unknown]); // Added is Changed too.
        expect_visibility(&app, a, true);
        expect_visibility(&app, b, false);
        assert!(!app.world().get::<InheritedVisibility>(child).unwrap().get());
        app.update();
        expect_changes(&app, &[]);

        app.world_mut()
            .resource_mut::<crate::player::LocalPlayer>()
            .render_eye = -glam::Vec3::X;
        app.update();
        expect_changes(&app, &[a, b]);
        expect_visibility(&app, a, false);
        expect_visibility(&app, b, true);
        assert!(app.world().get::<InheritedVisibility>(child).unwrap().get());
        for entity in [both, empty, unknown] {
            expect_visibility(&app, entity, true);
        }
        app.update();
        expect_changes(&app, &[]);
    }
}
