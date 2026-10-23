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
var removed = api.row(row, undefined);
csrEI.call("csr.HudLayoutFixture", removed, !row._visible,
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

#[test]
fn scoreboard_heading_scores_and_both_roster_headers_survive_refresh() {
    let calls = run(r#"
function near(a, b) { return Math.abs(a - b) < 0.12; }
function heading(parent, name, depth, fieldName, y) {
    var clip = parent.createEmptyMovieClip(name, depth);
    clip.createTextField(fieldName, 1, 0, 0, 150, 18);
    clip[fieldName].text = name;
    clip._y = y;
}
function header(parent, name, depth) {
    var clip = parent.createEmptyMovieClip(name, depth);
    var labels = ["Player_Label", "CT_money_header", "CT_Kills_Header", "CT_Death_Header", "CT_Ping_Header"];
    var i = 0;
    while (i < labels.length) {
        heading(clip, labels[i], i + 1, "Text", 6);
        i += 1;
    }
}
function emblem(parent, name, depth, y) {
    var clip = parent.createEmptyMovieClip(name, depth);
    clip.beginFill(0xffffff);
    clip.moveTo(0, 0); clip.lineTo(30, 0); clip.lineTo(30, 30);
    clip.lineTo(0, 30); clip.lineTo(0, 0); clip.endFill();
    clip._x = 77.6; clip._y = y;
}
function emblemCenter(clip, panel) {
    var bounds = clip.getBounds(panel);
    var center = (bounds.yMin + bounds.yMax) / 2;
    return center;
}
function checkScoreboardLayout() {
    var api = _global.csrScoreboard;
    var panel = _root.createEmptyMovieClip("layoutFixture", 402);
    header(panel, "CT_Headers", 1);
    // Pre-existing strip models a subsequent presentation. Dynamic empty
    // clips cannot model SWF symbol duplication; root checks that with Legacy.
    header(panel, "csrTHeaders", 4000);
    panel.CT_Headers._x = 214;
    panel.CT_Headers._y = 59;
    heading(panel, "CT_TeamName", 2, "CT_TeamNameText", 316);
    heading(panel, "T_TeamName", 3, "T_TeamNameText", 354);
    heading(panel, "CT_Score", 4, "CT_Score", 356);
    heading(panel, "T_Score", 5, "T_Score", 344);
    // Visible text has different local offsets from the clip origins.
    panel.CT_Score.CT_Score._y = -40;
    panel.T_Score.T_Score._y = 12;
    var bg = panel.createEmptyMovieClip("bg_scoreboard", 8);
    bg._x = 327.1; bg._y = 0.8;
    // The installed Standard background's unnamed depth-8 group is separate
    // from both score clips and has a non-unit vertical scale.
    var logos = bg.createEmptyMovieClip("fixtureUnnamedLogos", 8);
    logos._x = -70.3; logos._y = 70.25; logos._yscale = 98.35052490234375;
    emblem(logos, "DefaultIconCT", 1, 251.5);
    emblem(logos, "DefaultIconT", 3, 287.8);
    emblem(logos, "CTLogo", 5, 249.35);
    emblem(logos, "TLogo", 7, 285.3);
    var decoration = bg.createEmptyMovieClip("fixtureDecoration", 5);
    var sides = ["CT", "T"];
    var i = 0;
    while (i < 2) {
        var side = sides[i];
        var table = panel.createEmptyMovieClip(side + "_Scoretable", 6 + i);
        var row = table.createEmptyMovieClip(side + "_ScoreRow_0", 1);
        row.createTextField("Kills", 1, 0, 0, 40, 20);
        row.Kills.textColor = i == 0 ? 0x8acaff : 0xf2d373;
        table._y = i == 0 ? 83 : 388;
        i += 1;
    }
    var ok = api.layout(panel);
    var ct = panel.CT_Score._y + panel.CT_Score.CT_Score._y + panel.CT_Score.CT_Score._height / 2;
    var ctName = panel.CT_TeamName._y + panel.CT_TeamName.CT_TeamNameText._y + panel.CT_TeamName.CT_TeamNameText._height / 2;
    var t = panel.T_Score._y + panel.T_Score.T_Score._y + panel.T_Score.T_Score._height / 2;
    var tName = panel.T_TeamName._y + panel.T_TeamName.T_TeamNameText._y + panel.T_TeamName.T_TeamNameText._height / 2;
    csrEI.call("csr.HudLayoutFixture", ok,
        near(ct, ctName), near(t, tName),
        near(emblemCenter(logos.DefaultIconCT, panel), ctName),
        near(emblemCenter(logos.DefaultIconT, panel), tName),
        logos.DefaultIconCT._visible, logos.DefaultIconT._visible,
        !logos.CTLogo._visible, !logos.TLogo._visible, decoration._visible,
        panel.csrTHeaders._visible, near(panel.csrTHeaders._x, 214),
        near(panel.csrTHeaders._y, 364), !panel.csrTHeaders.CT_Ping_Header._visible,
        panel.CT_Headers.Player_Label.Text.textColor == 0x8acaff,
        panel.csrTHeaders.CT_Kills_Header.Text.textColor == 0xf2d373);
    var strip = panel.csrTHeaders;
    var scoreY = panel.CT_Score._y;
    ok = api.layout(panel);
    csrEI.call("csr.HudLayoutFixture", ok, panel.csrTHeaders == strip,
        near(panel.CT_Score._y, scoreY), near(panel.CT_Scoretable._y, 83),
        near(panel.T_Scoretable._y, 388),
        near(emblemCenter(logos.DefaultIconCT, panel), ctName),
        near(emblemCenter(logos.DefaultIconT, panel), tName));
}
checkScoreboardLayout();
"#, 1920, 1080);
    assert_eq!(calls.len(), 2, "missing layout receipt: {calls:?}");
    assert!(calls.iter().flatten().all(|value| value == &Value::Bool(true)),
        "scoreboard heading/header refresh: {calls:?}");
}
