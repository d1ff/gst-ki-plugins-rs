use glib::prelude::*;

mod imp;

glib::wrapper! {
    pub struct Remap(ObjectSubclass<imp::Remap>) @extends gst_base::BaseTransform, gst::Element, gst::Object;
}

unsafe impl Send for Remap {}
unsafe impl Sync for Remap {}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "remap",
        gst::Rank::Primary,
        Remap::static_type(),
    )
}
