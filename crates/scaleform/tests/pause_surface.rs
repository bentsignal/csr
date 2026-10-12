use ruffle_core::external::{ExternalInterfaceProvider, Value};
use ruffle_core::{FloatDuration, PlayerBuilder};
use std::{cell::RefCell, rc::Rc};

struct Source(String);
impl rascal::SourceProvider for Source {
    fn load(&self, path: &str) -> std::io::Result<String> {
        if path == "synthetic.as" {
            Ok(self.0.clone())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                path.to_string(),
            ))
        }
    }
    fn is_file(&self, path: &str) -> bool {
        path == "synthetic.as"
    }
}
struct Sink(Rc<RefCell<Vec<Vec<Value>>>>);
impl ExternalInterfaceProvider for Sink {
    fn call_method(
        &self,
        _: &mut ruffle_core::context::UpdateContext<'_>,
        name: &str,
        args: &[Value],
    ) -> Value {
        if name == "OwnedPauseSurface.Visibility" {
            self.0.borrow_mut().push(args.to_vec());
        }
        Value::Undefined
    }
    fn on_callback_available(&self, _: &str) {}
    fn get_id(&self) -> Option<String> {
        None
    }
}
#[test]
fn passive_pause_surface_reports_owned_synthetic_clip_properties() {
    let shim = include_str!("../src/shim.as").to_string();
    // Only our own bookkeeping source is exercised. Null backends serve no
    // game files; the generated root and child are asset-free MovieClips.
    let synthetic = r#"
_root._visible = true;
_root._alpha = 100;
_global.csrBindings.samplePauseSurface("23", "2", "1", "_level0", "");
_root.createEmptyMovieClip("syntheticOptions", 1);
_root.syntheticOptions._visible = true;
_root.syntheticOptions._alpha = 100;
_global.csrBindings.samplePauseSurface("23", "2", "2", "_level0", "_level0.syntheticOptions");
_root.syntheticOptions._visible = false;
_global.csrBindings.samplePauseSurface("23", "3", "3", "_level0", "_level0.syntheticOptions");
_root._visible = false;
_root._alpha = 50;
_global.csrBindings.samplePauseSurface("24", "4", "4", "_level0", "_level0.syntheticOptions");
_global.csrBindings.samplePauseSurface("24", "4", "5", "_level99", "_level98");
"#;
    let mut builder = rascal::ProgramBuilder::new(Source(shim + synthetic))
        .with_compile_options(rascal::CompileOptions::default().with_swf_version(10));
    builder.add_script("synthetic.as");
    let program = builder.build().unwrap();
    let data = program
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
    assert_eq!(calls.len(), 5);
    for (index, args) in calls.iter().enumerate() {
        assert_eq!(args.len(), 11);
        assert_eq!(args[2], Value::String((index + 1).to_string()));
        println!("sample {index}: {args:?}");
    }
    let stamps = [
        ["23", "2", "1", "_level0", ""],
        ["23", "2", "2", "_level0", "_level0.syntheticOptions"],
        ["23", "3", "3", "_level0", "_level0.syntheticOptions"],
        ["24", "4", "4", "_level0", "_level0.syntheticOptions"],
        ["24", "4", "5", "_level99", "_level98"],
    ];
    for (args, stamp) in calls.iter().zip(stamps) {
        assert_eq!(&args[..5], &stamp.map(Value::from));
    }
    assert_eq!(
        &calls[0][5..],
        &[
            true.into(),
            true.into(),
            100.0.into(),
            false.into(),
            Value::Undefined,
            Value::Undefined
        ]
    );
    assert_eq!(
        &calls[1][5..],
        &[
            true.into(),
            true.into(),
            100.0.into(),
            true.into(),
            true.into(),
            100.0.into()
        ]
    );
    assert_eq!(
        &calls[2][5..],
        &[
            true.into(),
            true.into(),
            100.0.into(),
            true.into(),
            false.into(),
            100.0.into()
        ]
    );
    assert_eq!(
        &calls[3][5..],
        &[
            true.into(),
            false.into(),
            50.0.into(),
            true.into(),
            false.into(),
            100.0.into()
        ]
    );
    assert_eq!(
        &calls[4][5..],
        &[
            false.into(),
            Value::Undefined,
            Value::Undefined,
            false.into(),
            Value::Undefined,
            Value::Undefined
        ]
    );
    println!(
        "Asset-free sampler protocol: 5 cases passed; no assets, GPU, clipboard or game/config writes."
    );
}
