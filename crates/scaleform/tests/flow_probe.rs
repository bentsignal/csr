//! Asset-free public display-list fixture. Authored here; execution belongs to root.
use ruffle_core::external::{ExternalInterfaceProvider, Value};
use ruffle_core::{FloatDuration, PlayerBuilder};
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
        if name == "csr.FlowFixture" { self.0.borrow_mut().push(args.to_vec()); }
        Value::Undefined
    }
    fn on_callback_available(&self, _: &str) {}
    fn get_id(&self) -> Option<String> { None }
}

#[test]
fn public_text_probe_observes_hidden_ancestors_ambiguity_and_never_changes_controls() {
    let source = include_str!("../src/shim.as").to_owned() + include_str!("../src/flow_probe.as") + r#"
_root.createEmptyMovieClip("menu", 1);
var menu = _root.menu;
menu._x = 10;
menu._y = 20;
menu.createEmptyMovieClip("button", 1);
var button = menu.button;
menu.aliasOfButton = button;
button._x = 40;
button._y = 60;
button.createTextField("caption", 1, 8, 4, 160, 30);
button.caption.text = "Audio Settings";
var clickCount = 0;
button.onRelease = function() { clickCount += 1; };
_global.csrDiagnostics.flowProbe("1", ["_level0.menu", "_level99"], "_level0.menu", "AUDIO SETTINGS");
var first = _global.csrFlowProbe["1"];
csrEI.call("csr.FlowFixture", first.matches, first.visible0, first.visible1, first.complete,
    first.x, first.y, clickCount, button.caption.text, menu._x, button._y);
menu.createTextField("duplicate", 2, 200, 100, 160, 30);
menu.duplicate.text = "Audio Settings";
_global.csrDiagnostics.flowProbe("2", ["_level0.menu"], "_level0.menu", "AUDIO SETTINGS");
csrEI.call("csr.FlowFixture", _global.csrFlowProbe["2"].matches,
    _global.csrFlowProbe["1"] == undefined, clickCount);
menu._visible = false;
_global.csrDiagnostics.flowProbe("3", ["_level0.menu.button"], "_level0.menu", "AUDIO SETTINGS");
csrEI.call("csr.FlowFixture", _global.csrFlowProbe["3"].matches,
    _global.csrFlowProbe["3"].visible0, menu._visible, clickCount);
"#;
    let mut builder = rascal::ProgramBuilder::new(Source(source))
        .with_compile_options(rascal::CompileOptions::default().with_swf_version(10));
    builder.add_script("synthetic.as");
    let data = builder.build().unwrap().compile()
        .to_swf(&rascal::SwfOptions::default().with_stage_size(0., 0., 1920., 1080.)).unwrap();
    let movie = ruffle_core::tag_utils::SwfMovieData::from_data(
        &data, "file:///synthetic.swf".into(), None, None).unwrap();
    let calls = Rc::new(RefCell::new(vec![]));
    let player = PlayerBuilder::new().with_movie(movie).with_autoplay(true)
        .with_external_interface(Box::new(Sink(calls.clone()))).build();
    for _ in 0..3 { player.lock().unwrap().tick(FloatDuration::from_secs(0.1)); }
    let calls = calls.borrow();
    assert_eq!(calls.len(), 3);
    assert_eq!(&calls[0][..4], &[1.0.into(), true.into(), false.into(), true.into()]);
    assert_eq!(&calls[0][4..], &[138.0.into(), 99.0.into(), 0.0.into(),
        "Audio Settings".into(), 10.0.into(), 60.0.into()]);
    assert_eq!(calls[1], vec![2.0.into(), true.into(), 0.0.into()]);
    assert_eq!(calls[2], vec![0.0.into(), false.into(), false.into(), 0.0.into()]);
}
