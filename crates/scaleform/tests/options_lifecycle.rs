//! Execute our real shim with independently authored, in-memory movies. These
//! fixtures establish adapter semantics, not the installed movie's call pattern.
use ruffle_core::backend::navigator::{
    ErrorResponse, NavigationMethod, NavigatorBackend, NullExecutor, NullSpawner,
    OwnedFuture, Request, SuccessResponse, create_fetch_error,
};
use ruffle_core::external::{ExternalInterfaceProvider, Value};
use ruffle_core::indexmap::IndexMap;
use ruffle_core::loader::Error as LoadError;
use ruffle_core::socket::{ConnectionState, SocketAction, SocketHandle};
use ruffle_core::{FloatDuration, PlayerBuilder};
use std::{borrow::Cow, cell::RefCell, rc::Rc, time::Duration};
use url::Url;

struct Source(String);
impl rascal::SourceProvider for Source {
    fn load(&self, path: &str) -> std::io::Result<String> {
        if path == "fixture.as" {
            Ok(self.0.clone())
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, path))
        }
    }
    fn is_file(&self, path: &str) -> bool { path == "fixture.as" }
}
fn compile(source: String) -> Vec<u8> {
    let mut builder = rascal::ProgramBuilder::new(Source(source))
        .with_compile_options(rascal::CompileOptions::default().with_swf_version(10));
    builder.add_script("fixture.as");
    builder.build().unwrap().compile().to_swf(&rascal::SwfOptions::default()).unwrap()
}

// Serve exactly one generated child. No disk, network, install or GPU backend.
struct Response { bytes: Option<Vec<u8>> }
impl SuccessResponse for Response {
    fn url(&self) -> Cow<'_, str> { Cow::Borrowed("file:///synthetic-options.swf") }
    fn set_url(&mut self, _: String) {}
    fn body(self: Box<Self>) -> OwnedFuture<Vec<u8>, LoadError> {
        Box::pin(async move { Ok(self.bytes.unwrap_or_default()) })
    }
    fn text_encoding(&self) -> Option<&'static encoding_rs::Encoding> { None }
    fn status(&self) -> u16 { 200 }
    fn redirected(&self) -> bool { false }
    fn next_chunk(&mut self) -> OwnedFuture<Option<Vec<u8>>, LoadError> {
        let bytes = self.bytes.take();
        Box::pin(async move { Ok(bytes) })
    }
    fn expected_length(&self) -> Result<Option<u64>, LoadError> {
        Ok(self.bytes.as_ref().map(|bytes| bytes.len() as u64))
    }
}
struct Movies { child: Vec<u8>, spawner: NullSpawner }
impl NavigatorBackend for Movies {
    fn navigate_to_url(&self, _: &str, _: &str, _: Option<(NavigationMethod, IndexMap<String, String>)>) {
        panic!("fixture must not navigate");
    }
    fn fetch(&self, request: Request) -> OwnedFuture<Box<dyn SuccessResponse>, ErrorResponse> {
        let result = if self.resolve_url(request.url()).ok().is_some_and(|url|
            url.as_str() == "file:///synthetic-options.swf") {
            Ok(Box::new(Response { bytes: Some(self.child.clone()) }) as Box<dyn SuccessResponse>)
        } else {
            Err(create_fetch_error(request.url(), "fixture serves only its generated child"))
        };
        Box::pin(async move { result })
    }
    fn resolve_url(&self, url: &str) -> Result<Url, url::ParseError> {
        Url::parse("file:///").unwrap().join(url)
    }
    fn spawn_future(&mut self, future: OwnedFuture<(), LoadError>) { self.spawner.spawn_local(future); }
    fn pre_process_url(&self, url: Url) -> Url { url }
    fn connect_socket(&mut self, _: String, _: u16, _: Duration, handle: SocketHandle,
        _: async_channel::Receiver<Vec<u8>>, sender: async_channel::Sender<SocketAction>) {
        sender.try_send(SocketAction::Connect(handle, ConnectionState::Failed)).unwrap();
    }
}

type Events = Rc<RefCell<Vec<(String, Vec<Value>)>>>;
struct Sink(Events);
impl ExternalInterfaceProvider for Sink {
    fn call_method(&self, _: &mut ruffle_core::context::UpdateContext<'_>, name: &str, args: &[Value]) -> Value {
        self.0.borrow_mut().push((name.into(), args.to_vec()));
        Value::Undefined
    }
    fn on_callback_available(&self, _: &str) {}
    fn get_id(&self) -> Option<String> { None }
}

#[test]
fn actual_options_bindings_keep_old_functions_but_rebind_retained_api_lookups() {
    let root = format!("var csrRootMovie = \"synthetic-options.swf\"; var csrRootLevel = 21;\n{}\n{}",
        include_str!("../src/shim.as"), r#"
// The child is loaded through ordinary loadMovieNum. Do not replace csrResolve
// or csrMakeProxy: execute the same bindings and level resolution as production.
csrEI.call("fixture.Assert", "not loaded yet", _global.csrBindings.bindOwnedOptions("_level21", "OwnedOptions1") == false);
this.onEnterFrame = function() {
    if (_level21.fixtureReady != true) { return; }
    delete this.onEnterFrame;
    var root = _level21;
    var api = root.gameAPI;
    var retainedAPI = api;
    var bindings = _global.csrBindings;
    csrEI.call("fixture.Assert", "first binding", bindings.bindOwnedOptions("_level21", "OwnedOptions1"));
    var oldCancel = api.OnCancel;
    var oldUnload = api.OnUnload;
    api.OnCancel();
    api.OnUnload(root);
    csrEI.call("fixture.Assert", "replacement binding", bindings.bindOwnedOptions("_level21", "OwnedOptions2"));
    // A retained FUNCTION belongs to A. A property lookup on a retained API
    // OBJECT belongs to B. Neither case proves how installed Options calls it.
    oldCancel();
    oldUnload(root);
    retainedAPI.OnCancel();
    retainedAPI.OnUnload(root);
    bindings.cancelOwnedOptions("OwnedOptions1");
    bindings.cancelOwnedOptions("OwnedOptions2");
    api.GetConvarString("fixture_only");
    csrEI.call("fixture.Assert", "same cached API", root.gameAPI == api && api == _global.csrApi("OptionsMenu"));

    var liveCancel = api.OnCancel;
    var liveUnload = api.OnUnload;
    var badPaths = ["", "mc:_level21", "_level99", "_level21.Panel", "obj:0", "_global.OptionsMenu", undefined, null, 21, new Object()];
    var i = 0;
    while (i < badPaths.length) {
        csrEI.call("fixture.Assert", "invalid path " + i, bindings.bindOwnedOptions(badPaths[i], "OwnedOptions3") == false);
        i = i + 1;
    }
    var badNames = ["", "OptionsMenu", "OwnedOptions", "OwnedOptions01", "OwnedOptions-1", "OwnedOptions1.OnCancel", "OwnedOptions18446744073709551616", undefined, 1];
    i = 0;
    while (i < badNames.length) {
        csrEI.call("fixture.Assert", "invalid receiver " + i, bindings.bindOwnedOptions("_level21", badNames[i]) == false);
        bindings.cancelOwnedOptions(badNames[i]);
        i = i + 1;
    }
    root.elementName = "WrongElement";
    csrEI.call("fixture.Assert", "wrong element", bindings.bindOwnedOptions("_level21", "OwnedOptions3") == false);
    root.elementName = "OptionsMenu";
    root.gameAPI = new Object();
    csrEI.call("fixture.Assert", "wrong API", bindings.bindOwnedOptions("_level21", "OwnedOptions3") == false);
    root.gameAPI = api;
    csrEI.call("fixture.Assert", "failure leaves live functions intact", api.OnCancel == liveCancel && api.OnUnload == liveUnload);
    api.OnCancel();
    api.OnUnload(root);
    csrEI.call("fixture.Done");
};
"#);
    let child = compile(r#"
this.elementName = "OptionsMenu";
this.gameAPI = _global.csrApi("OptionsMenu");
this.fixtureReady = true;
stop();
"#.into());
    let data = compile(root);
    let movie = ruffle_core::tag_utils::SwfMovieData::from_data(
        &data, "file:///fixture.swf".into(), None, None).unwrap();
    let events: Events = Default::default();
    let mut executor = NullExecutor::new();
    let player = PlayerBuilder::new().with_movie(movie).with_autoplay(true)
        .with_navigator(Movies { child, spawner: executor.spawner() })
        .with_external_interface(Box::new(Sink(events.clone()))).build();
    for _ in 0..30 {
        player.lock().unwrap().tick(FloatDuration::from_secs(0.1));
        executor.run();
        if events.borrow().iter().any(|(name, _)| name == "fixture.Done") { break; }
    }
    let events = events.borrow();
    assert!(events.iter().any(|(name, _)| name == "fixture.Done"), "generated clip did not execute: {events:?}");
    let assertions: Vec<_> = events.iter().filter(|(name, _)| name == "fixture.Assert").collect();
    assert_eq!(assertions.len(), 26);
    for (_, args) in assertions {
        assert!(matches!(args.as_slice(), [Value::String(_), Value::Bool(true)]), "{args:?}");
    }
    let callbacks: Vec<_> = events.iter().filter(|(name, _)| !name.starts_with("fixture.")).cloned().collect();
    let unload = vec![Value::String("mc:_level21".into())];
    assert_eq!(callbacks, vec![
        ("OwnedOptions1.OnCancel".into(), vec![]),
        ("OwnedOptions1.OnUnload".into(), unload.clone()),
        ("OwnedOptions1.OnCancel".into(), vec![]),
        ("OwnedOptions1.OnUnload".into(), unload.clone()),
        ("OwnedOptions2.OnCancel".into(), vec![]),
        ("OwnedOptions2.OnUnload".into(), unload.clone()),
        ("OwnedOptions1.OnCancel".into(), vec![]),
        ("OwnedOptions2.OnCancel".into(), vec![]),
        ("OptionsMenu.GetConvarString".into(), vec![Value::String("fixture_only".into())]),
        ("OwnedOptions2.OnCancel".into(), vec![]),
        ("OwnedOptions2.OnUnload".into(), unload),
    ]);
}
