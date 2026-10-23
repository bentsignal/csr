//! Asset-free display-list regressions. Source only in the HUD resource slot;
//! root must execute these alongside the existing raster/readiness tests.
use ruffle_core::external::{ExternalInterfaceProvider, Value};
use ruffle_core::{FloatDuration, PlayerBuilder, StageAlign, StageScaleMode};
use std::{cell::RefCell, rc::Rc};

struct Source(String);
impl rascal::SourceProvider for Source {
    fn load(&self, path: &str) -> std::io::Result<String> {
        if path == "synthetic.as" { Ok(self.0.clone()) }
        else { Err(std::io::Error::new(std::io::ErrorKind::NotFound, path.to_owned())) }
    }
    fn is_file(&self, path: &str) -> bool { path == "synthetic.as" }
}
struct Sink(Rc<RefCell<Vec<Vec<Value>>>>);
impl ExternalInterfaceProvider for Sink {
    fn call_method(&self, _: &mut ruffle_core::context::UpdateContext<'_>, name: &str, args: &[Value]) -> Value {
        if name == "GameInterface.GetConvarNumber" && args.first() == Some(&Value::String("hud_scaling".into())) {
            return scaleform::HUD_SCALING.into();
        }
        if name == "csr.HudLayoutFixture" { self.0.borrow_mut().push(args.to_vec()); }
        Value::Undefined
    }
    fn on_callback_available(&self, _: &str) {}
    fn get_id(&self) -> Option<String> { None }
}

fn run(body: &str, width: u32, height: u32) -> Vec<Vec<Value>> {
    let source = include_str!("../src/shim.as").to_owned()
        + include_str!("../src/hud_layout.as")
        + include_str!("../src/death_notice.as")
        + include_str!("../src/scoreboard.as") + body;
    let mut builder = rascal::ProgramBuilder::new(Source(source))
        .with_compile_options(rascal::CompileOptions::default().with_swf_version(10));
    builder.add_script("synthetic.as");
    let data = builder.build().unwrap().compile()
        .to_swf(&rascal::SwfOptions::default().with_stage_size(0., 0., 1280., 720.)).unwrap();
    let movie = ruffle_core::tag_utils::SwfMovieData::from_data(
        &data, "file:///synthetic.swf".into(), None, None).unwrap();
    let calls = Rc::new(RefCell::new(vec![]));
    let player = PlayerBuilder::new().with_movie(movie).with_autoplay(true)
        .with_viewport_dimensions(width, height, 1.0)
        .with_scale_mode(StageScaleMode::NoScale, true)
        .with_align(StageAlign::TOP | StageAlign::LEFT, true)
        .with_external_interface(Box::new(Sink(calls.clone()))).build();
    for _ in 0..3 { player.lock().unwrap().tick(FloatDuration::from_secs(0.1)); }
    let result = calls.borrow().clone();
    result
}

#[test]
fn bottom_components_scale_about_their_corners_once_and_not_about_content_bounds() {
    for (width, height) in [(1920, 1080), (1601, 901), (1440, 1080)] {
        let calls = run(r#"
function near(a, b) { return Math.abs(a - b) < 0.12; }
function rectangle(parent, name, depth, width, height) {
    var clip = parent.createEmptyMovieClip(name, depth);
    clip.beginFill(0xffffff, 100);
    clip.moveTo(0, 0); clip.lineTo(width, 0); clip.lineTo(width, height);
    clip.lineTo(0, height); clip.lineTo(0, 0); clip.endFill();
    return clip;
}
var left = _root.createEmptyMovieClip("health", 1);
var right = _root.createEmptyMovieClip("ammo", 2);
rectangle(left, "ink", 1, 40, 20);
rectangle(right, "ink", 1, 40, 20);
left.onResize = function() {
    this.resetObserved = this._xscale == 100 && this._yscale == 100 && this._x == 0 && this._y == 0;
    this.ink._x = 0; this.ink._y = Stage.height - 20;
};
right.onResize = function() {
    this.resetObserved = this._xscale == 100 && this._yscale == 100 && this._x == 0 && this._y == 0;
    this.ink._x = Stage.width - 40; this.ink._y = Stage.height - 20;
};
var turn = 0;
while (turn < 3) {
    _global.csrHudLayout.bottom(left, false);
    _global.csrHudLayout.bottom(right, true);
    var a = left.ink.getBounds(_root);
    var b = right.ink.getBounds(_root);
    csrEI.call("csr.HudLayoutFixture", left.resetObserved && right.resetObserved,
        near(a.xMin, 0) && near(a.yMax, Stage.height),
        near(b.xMax, Stage.width) && near(b.yMax, Stage.height),
        near(a.xMax - a.xMin, 34) && near(b.xMax - b.xMin, 34),
        near(a.yMax - a.yMin, 17) && near(b.yMax - b.yMin, 17));
    // Helper/template bounds must never feed back into the layout transform.
    if (turn == 0) { rectangle(right, "unrelatedTemplate", 2, 800, 200); }
    turn += 1;
}
var notices = _root.createEmptyMovieClip("notices", 3);
notices.createEmptyMovieClip("Panel", 1);
notices.Panel._y = 25;
_global.csrHudLayout.notices(notices);
var y = notices.Panel._y;
_global.csrHudLayout.notices(notices);
csrEI.call("csr.HudLayoutFixture", near(y, Stage.height * 52 * 0.85 / 720),
    near(notices.Panel._y, y), y > Stage.height * 40 * 0.85 / 720);
"#, width, height);
        assert_eq!(calls.len(), 4);
        for call in calls { assert!(call.iter().all(|value| value == &Value::Bool(true)), "{width}x{height}: {call:?}"); }
    }
}

#[test]
fn notice_fit_uses_actual_text_parent_coordinates_and_retains_only_the_authored_right_inset() {
    let calls = run(r#"
function near(a, b) { return Math.abs(a - b) < 0.12; }
function box(parent, name, depth) {
    var clip = parent.createEmptyMovieClip(name, depth);
    clip.beginFill(0xffffff, 100);
    clip.moveTo(-400, 0); clip.lineTo(0, 0); clip.lineTo(0, 24);
    clip.lineTo(-400, 24); clip.lineTo(-400, 0); clip.endFill();
    return clip;
}
var panel = _root.createEmptyMovieClip("row", 1);
panel._x = 800; panel._xscale = 85; panel._yscale = 85;
box(panel, "IDTextBG", 1); box(panel, "PanelAttacker", 2); box(panel, "PanelVictim", 3);
var parent = panel.createEmptyMovieClip("TextPanel", 4);
parent._x = -10; parent._xscale = 150;
var widths = [40, 100, 200, 100];
var i = 0;
while (i < widths.length) {
    // Shared right-aligned endpoint in TextPanel space; variable names/icons
    // change only the span. Right inset40 in row space must remain40.
    var width = widths[i];
    var success = _global.csrDeathNotices.fitTextBackground(panel, parent, -20 - width, width, 0);
    var bg = panel.IDTextBG.getBounds(panel);
    var attacker = panel.PanelAttacker.getBounds(panel);
    var victim = panel.PanelVictim.getBounds(panel);
    csrEI.call("csr.HudLayoutFixture", success, near(bg.xMin, -80 - 1.5 * width), near(bg.xMax, 0),
        near(attacker.xMin, bg.xMin) && near(attacker.xMax, bg.xMax),
        near(victim.xMin, bg.xMin) && near(victim.xMax, bg.xMax));
    i += 1;
}
"#, 1920, 1080);
    assert_eq!(calls.len(), 4);
    for call in calls { assert!(call.iter().all(|value| value == &Value::Bool(true)), "{call:?}"); }
}

#[test]
fn scoreboard_rows_replace_template_values_and_hide_unowned_stats() {
    let calls = run(r#"
function checkScoreboardRows() {
var row = _root.createEmptyMovieClip("fixtureRow", 400);
row.createEmptyMovieClip("Player_Name", 1);
row.Player_Name.createTextField("Player_Name", 1, 0, 0, 300, 20);
row.createTextField("Money", 2, 0, 0, 80, 20);
row.createTextField("Kills", 3, 0, 0, 40, 20);
row.createTextField("Death", 4, 0, 0, 40, 20);
row.createTextField("Assists", 5, 0, 0, 40, 20);
row.createEmptyMovieClip("Bomb", 6);
row.createEmptyMovieClip("Skull", 7);
row.createEmptyMovieClip("PlayerOutline", 8);
row.Assists.text = "999";
var api = _global.csrScoreboard;
var ok = api.row(row, ["<literal & name>", "C4", "$800", "3", "2", true]);
csrEI.call("csr.HudLayoutFixture", ok, row.Player_Name.Player_Name.text == "<literal & name>",
    row.Money.text == "$800", row.Kills.text == "3", row.Death.text == "2",
    row.Bomb._visible, !row.Skull._visible, row.PlayerOutline._visible, !row.Assists._visible);
ok = api.row(row, ["Enemy", "DEAD", "", "4", "3", false]);
csrEI.call("csr.HudLayoutFixture", ok, row.Money.text == "", !row.Bomb._visible,
    row.Skull._visible, !row.PlayerOutline._visible);
csrEI.call("csr.HudLayoutFixture", api.row(row, undefined), !row._visible,
    !api.row(undefined, ["Missing", "ALIVE", "", "0", "0", false]));
}
checkScoreboardRows();
"#, 1920, 1080);
    assert_eq!(calls.len(), 3);
    assert!(calls.iter().flatten().all(|value| value == &Value::Bool(true)),
        "scoreboard row receipts (local, enemy, remove/reject): {calls:?}");
}

/// Diagnose the actual return channel independently from text side effects.
/// Keep exact Boolean expectations: undefined must never pass as success.
#[test]
fn scoreboard_helper_return_channel_preserves_callers_values() {
    let calls = run(r#"
function checkScoreboardReturnChannel() {
    var api = _global.csrScoreboard;
    var fixture = _root.createEmptyMovieClip("csrReturnFixture", 401);
    fixture.createTextField("Money", 1, 0, 0, 100, 20);
    fixture.createEmptyMovieClip("Nested", 2);
    fixture.Nested.createEmptyMovieClip("Leaf", 1);
    // The same declaration/call form as the failing row fixture, with no
    // installed timeline, recursive traversal, or inline receipt expression.
    var literal = function() { return true; };
    var constantResult = literal();
    var writeResult = api.write(fixture.Money, "$800");
    var rejectedResult = api.write(undefined, "missing");
    csrEI.call("csr.HudLayoutFixture", constantResult, writeResult,
        rejectedResult === false, fixture.Money.text == "$800");
    var afterTimeline = function() {
        fixture.gotoAndStop("UnownedMissingLabel");
        return true;
    };
    var timelineResult = afterTimeline();
    csrEI.call("csr.HudLayoutFixture", timelineResult);
    var afterFreeze = function() {
        api.freeze(fixture, 0);
        return true;
    };
    var freezeResult = afterFreeze();
    csrEI.call("csr.HudLayoutFixture", freezeResult);
    var afterOnly = function() {
        api.only(fixture, {Money:true});
        return true;
    };
    var onlyResult = afterOnly();
    csrEI.call("csr.HudLayoutFixture", onlyResult, fixture.Money._visible,
        !fixture.Nested._visible);
}
checkScoreboardReturnChannel();
"#, 1920, 1080);
    assert_eq!(calls.len(), 4, "missing helper receipt: {calls:?}");
    assert!(calls.iter().flatten().all(|value| value == &Value::Bool(true)),
        "scoreboard helper receipts (literal/write/reject, timeline, freeze, only): {calls:?}");
}
