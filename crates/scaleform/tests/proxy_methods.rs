//! Asset-free exercise of our own AS2 wrapper cache in the actual AVM1 VM.
use ruffle_core::external::{ExternalInterfaceProvider, Value};
use ruffle_core::{FloatDuration, PlayerBuilder};
use std::{cell::RefCell, rc::Rc};

struct Source(String);
impl rascal::SourceProvider for Source {
    fn load(&self, path: &str) -> std::io::Result<String> {
        if path == "synthetic.as" {
            Ok(self.0.clone())
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, path))
        }
    }
    fn is_file(&self, path: &str) -> bool {
        path == "synthetic.as"
    }
}
type Records = Rc<RefCell<Vec<(String, Vec<Value>)>>>;
struct Sink(Records);
impl ExternalInterfaceProvider for Sink {
    fn call_method(
        &self,
        _: &mut ruffle_core::context::UpdateContext<'_>,
        name: &str,
        args: &[Value],
    ) -> Value {
        let mut calls = self.0.borrow_mut();
        let next = calls.iter().filter(|(old, _)| old == name).count() + 1;
        calls.push((name.to_string(), args.to_vec()));
        Value::Number(next as f64)
    }
    fn on_callback_available(&self, _: &str) {}
    fn get_id(&self) -> Option<String> {
        None
    }
}

#[test]
fn cached_native_methods_keep_dynamic_values_arguments_and_old_owner_routes() {
    let synthetic = r#"
var proxy = _global.csrMakeProxy("Synthetic");
var original = proxy.GetValue;
var first = proxy.GetValue("first", 2, false);
var second = proxy.GetValue("second", 3, true);
var repeated = proxy.GetValue == original;
var enumeration = 0;
for (var key in proxy) {
    if (key != "__resolve") { enumeration = enumeration + 1; }
}
var warm = proxy.Warm;
var i = 0;
while (i < 300) {
    var ignored = proxy["Unique" + i];
    i = i + 1;
}
var overflow = proxy.Unique299;
var bounded = proxy.Unique299 != overflow;
var stillWarm = proxy.Warm == warm;
_global.csrBindings.bindOwnedApi("LobbyAPI", "OwnedLobby1");
var oldProxy = _global.LobbyAPI;
var oldLaunch = oldProxy.LaunchGame;
_global.csrBindings.bindOwnedApi("LobbyAPI", "OwnedLobby2");
oldLaunch("old");
_global.LobbyAPI.LaunchGame("current");
csrEI.call("Synthetic.Results", first, second, repeated, enumeration, bounded, stillWarm);
"#;
    let mut builder = rascal::ProgramBuilder::new(Source(
        include_str!("../src/shim.as").to_string() + synthetic,
    ))
    .with_compile_options(rascal::CompileOptions::default().with_swf_version(10));
    builder.add_script("synthetic.as");
    let data = builder
        .build()
        .unwrap()
        .compile()
        .to_swf(&rascal::SwfOptions::default())
        .unwrap();
    let movie = ruffle_core::tag_utils::SwfMovieData::from_data(
        &data,
        "file:///synthetic.swf".into(),
        None,
        None,
    )
    .unwrap();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let player = PlayerBuilder::new()
        .with_movie(movie)
        .with_autoplay(true)
        .with_external_interface(Box::new(Sink(calls.clone())))
        .build();
    for _ in 0..3 {
        player.lock().unwrap().tick(FloatDuration::from_secs(0.1));
    }
    let calls = calls.borrow();
    let native: Vec<_> = calls
        .iter()
        .filter(|(name, _)| name == "Synthetic.GetValue")
        .collect();
    assert_eq!(
        native.len(),
        2,
        "wrapper cache must not cache getter values"
    );
    assert_eq!(
        native[0].1,
        vec![
            Value::String("first".into()),
            Value::Number(2.),
            Value::Bool(false)
        ]
    );
    assert_eq!(
        native[1].1,
        vec![
            Value::String("second".into()),
            Value::Number(3.),
            Value::Bool(true)
        ]
    );
    assert!(
        calls
            .iter()
            .any(|(name, args)| name == "OwnedLobby1.LaunchGame"
                && args == &[Value::String("old".into())])
    );
    assert!(
        calls
            .iter()
            .any(|(name, args)| name == "OwnedLobby2.LaunchGame"
                && args == &[Value::String("current".into())])
    );
    let result = calls
        .iter()
        .find(|(name, _)| name == "Synthetic.Results")
        .unwrap();
    assert_eq!(
        result.1,
        vec![
            Value::Number(1.),
            Value::Number(2.),
            Value::Bool(true),
            Value::Number(0.),
            Value::Bool(true),
            Value::Bool(true)
        ]
    );
}
