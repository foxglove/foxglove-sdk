fn main() {
    let widget_js = "python/foxglove/notebook/static/widget.js";
    if !std::path::Path::new(widget_js).exists() {
        panic!("error: {widget_js} not found, run `yarn build`");
    }
}
