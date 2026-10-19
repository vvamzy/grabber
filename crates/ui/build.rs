// Embed a Common-Controls v6 manifest dependency so the rebar/toolbar/tree/
// list/status bar get the modern themed look instead of the classic Win2000
// rendering. Passed straight to the linker (not through a shell), so quoting
// is safe.
fn main() {
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' \
         name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
         processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );
}
