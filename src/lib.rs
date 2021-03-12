mod remap;

fn plugin_init(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    remap::register(plugin)?;
    Ok(())
}

gst::plugin_define!(
    kiplugins,
    "KnotInspector plugins",
    plugin_init,
    "1.0.0",
    "MIT/X11",
    "KnotInspector Plugins",
    "KnotInspector Plugins",
    "NA",
    "2021-03-09"
);
