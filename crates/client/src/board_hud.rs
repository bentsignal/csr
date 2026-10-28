//! Pack-driven trick scoring and contextual session-marker help.
use bevy::prelude::*;
#[derive(Component, Clone, Copy, PartialEq)]
pub(crate) enum Part {
    Trick,
    Score,
    Marker,
}
pub(crate) fn update(
    mut commands: Commands,
    world: Res<crate::player::SimWorld>,
    local: Res<crate::player::LocalPlayer>,
    art: Res<crate::ui_assets::UiAssets>,
    menu: Res<crate::menu::Menu>,
    playtest: Option<Res<crate::playtest::Panel>>,
    mut labels: Query<(&Part, &mut Text, &mut Node, &mut TextColor)>,
) {
    if labels.is_empty() {
        for (part, size, bottom, top) in [
            (Part::Trick, 28., Some(130.), None),
            (Part::Score, 38., Some(165.), None),
            (Part::Marker, 21., None, Some(260.)),
        ] {
            commands.spawn((
                part,
                Text::new(""),
                TextFont {
                    font: art.bold.clone().into(),
                    font_size: FontSize::Px(size),
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(30),
                    bottom: bottom.map_or(Val::Auto, px),
                    top: top.map_or(Val::Auto, px),
                    ..default()
                },
                TextShadow {
                    offset: Vec2::splat(2.),
                    color: Color::BLACK,
                },
            ));
        }
        return;
    }
    let pose = world
        .0
        .player(local.id)
        .filter(|p| {
            p.alive
                && !menu.open()
                && !playtest.as_ref().is_some_and(|p| p.open)
                && local.spectating.is_none()
        })
        .and_then(|p| p.mechanic.as_ref()?.external.as_ref())
        .map(|s| &s.pose);
    for (part, mut text, mut node, mut color) in &mut labels {
        let content = pose.map_or_else(String::new, |p| match part {
            Part::Trick => {
                if p.score > 0 {
                    p.trick.clone()
                } else {
                    String::new()
                }
            }
            Part::Score => {
                if p.score > 0 {
                    format!("{}  ×{:.0}", p.score, p.multiplier.max(1.))
                } else {
                    String::new()
                }
            }
            Part::Marker => {
                if p.marker.visible {
                    format!(
                        "SESSION MARKER\nD-pad Down: {}\nD-pad Up: {}{}",
                        if p.marker.can_place {
                            "Place marker"
                        } else {
                            "Find level ground to place"
                        },
                        if p.marker.can_return {
                            "Hold to return"
                        } else {
                            "Place a marker first"
                        },
                        if p.marker.progress > 0. {
                            format!("  {}%", (p.marker.progress * 100.).round())
                        } else {
                            String::new()
                        }
                    )
                } else {
                    String::new()
                }
            }
        });
        node.display = if content.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        color.0 = if *part == Part::Score {
            Color::srgb(0.45, 0.87, 1.)
        } else {
            Color::WHITE
        };
        if text.0 != content {
            text.0 = content;
        }
    }
}
