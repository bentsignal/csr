"""Synthetic cc66e5e text, never captured game data or runtime proof.

Field order is from frame_report.rs; raw camera Debug shapes are checked against
the installed Bevy 0.19.1 camera types. These are local/view-target endpoints,
not per-bot observations or a whole-interval camera trace.
"""

ACTOR = ('Some(ActorContext { id: 1, life_epoch: 2, alive: true, '
         'team: CounterTerrorist, active_slot: 0, weapon_def_index: Some(7) })')
MATRIX = '[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]'
RECT = 'Some(URect { min: UVec2(0, 0), max: UVec2(2560, 1440) })'


def endpoint_lines(actor=ACTOR):
    player = ('perf player endpoint: endpoint_only=true role={role} requested_id=1 authoritative=' + actor +
              ' weapon_class=Some("weapon_ak47") '
              'weapon_content_id=Some(ContentId { origin: Csgo, kind: Weapon, name: "ak47" }) '
              'origin_source_inches=Some([-128.0, 64.5, 32.0]) view_angles_source_degrees=Some([-15.0, 90.0, 0.0])')
    # str.format would also interpret ActorContext's braces.
    return [player.replace('{role}', role) for role in ('local', 'view_target')] + [
        'perf view intent endpoint: endpoint_only=true render_eye_source_inches=[-128.0, 64.5, 96.0] '
        'render_angles_source_degrees=[-15.0, 90.0, 0.0]',
        'perf world camera endpoint: endpoint_only=true source=main_world_last interval_camera_stability=unproven '
        'candidates=1 entity=42v0 role_valid=true player_camera=true viewmodel_camera=false is_active=true order=0 '
        'transform_bevy=Transform { translation: Vec3(-3.2512, 2.4384, -1.6383), '
        'rotation: Quat(0.0, 0.0, 0.0, 1.0), scale: Vec3(1.0, 1.0, 1.0) } '
        f'global_matrix_bevy_cols={MATRIX} '
        'projection=Perspective(PerspectiveProjection { fov: 1.287, aspect_ratio: 1.7777778, near: 0.1, far: 1000.0 }) '
        'fov_vertical_horizontal_degrees=Some([73.7398, 106.2602]) '
        f'computed_clip_from_view_cols={MATRIX} target=Window(Primary) render_layers=None viewport=None '
        f'physical_viewport_rect={RECT} sub_camera_view=None',
    ]


def upgrade(text, *, endpoints=True, actor=ACTOR, resolution=(2560, 1440)):
    text = text.replace('noclip: false })', 'noclip: false, authoritative_local: ' + actor +
                        ', authoritative_view_target: ' + actor + ' })')
    text = text.replace('size: (1920, 1080)', f'size: {resolution}')
    lines = []
    for line in text.splitlines():
        if endpoints and 'perf simulation census:' in line:
            prefix = line.split('perf simulation census:', 1)[0]
            lines.extend(prefix + endpoint for endpoint in endpoint_lines(actor))
        lines.append(line)
    return '\n'.join(lines) + '\n'
