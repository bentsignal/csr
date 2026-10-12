// Temporary CI facade while the menu integrates the production leaf module.
#[path = "../src/pack_picker.rs"]
mod pack_picker;

// Compile the production entry point without opening a desktop picker.
const _: fn() -> pack_picker::PackPicker = pack_picker::PackPicker::start;
