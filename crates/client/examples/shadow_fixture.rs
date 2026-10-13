//! Asset-free pipeline proof. Run only in the coordinated GPU test slot.
fn main() {
    std::process::exit(client::shadow_diagnostic::fixture::run());
}
