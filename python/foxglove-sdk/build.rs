/**
 * Script to check if the widget.js file exists before building the Python package.
 * If the script does not find the widget.js file, it will panic and print an error message.
 * This script runs automatically as part of maturin's build process.
 */
fn main() {
    let widget_js = "python/foxglove/notebook/static/widget.js";
    if !std::path::Path::new(widget_js).exists() {
        panic!("error: {widget_js} not found, run `yarn build`");
    }
}
