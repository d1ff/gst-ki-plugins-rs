use glib::prelude::*;
use glib::subclass::prelude::*;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst::{gst_debug, gst_info};
use gst_base::subclass::prelude::*;
use opencv::{
    core::{self, Mat, ToOutputArray, UMat, UMatUsageFlags},
    imgcodecs, imgproc,
    prelude::*,
    types, Result,
};

use atomic_refcell::AtomicRefCell;
use once_cell::sync::Lazy;
use std::ffi::c_void;
use std::i32;
use std::sync::Mutex;

#[derive(Debug, Clone)]
struct Settings {
    mapx: String,
    mapy: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            mapx: String::new(),
            mapy: String::new(),
        }
    }
}

impl Settings {
    fn has_maps(&self) -> bool {
        !self.mapx.is_empty() && !self.mapy.is_empty()
    }
}

struct State {
    in_info: Option<gst_video::VideoInfo>,
    out_info: Option<gst_video::VideoInfo>,
    in_height: i32,
    in_width: i32,
    in_stride: usize,
    in_format: i32,
    out_height: i32,
    out_width: i32,
    out_stride: usize,
    out_format: i32,
    pad_sink_width: i32,
    pad_sink_height: i32,
    mapx: Mat,
    mapy: Mat,
}

impl Default for State {
    fn default() -> Self {
        Self {
            in_info: None,
            out_info: None,
            in_height: 0,
            in_width: 0,
            in_format: 0,
            in_stride: 0,
            out_height: 0,
            out_width: 0,
            out_format: 0,
            out_stride: 0,
            pad_sink_width: 0,
            pad_sink_height: 0,
            mapx: Mat::default().unwrap(),
            mapy: Mat::default().unwrap(),
        }
    }
}

impl State {
    fn set_in_info(&mut self, in_info: gst_video::VideoInfo) {
        self.in_height = in_info.height() as i32;
        self.in_width = in_info.width() as i32;
        self.in_format = cv_image_type_from_video_format(&in_info.format());
        self.in_stride = in_info.stride()[0] as usize;
        self.in_info = Some(in_info);
    }

    fn set_out_info(&mut self, out_info: gst_video::VideoInfo) {
        self.out_height = out_info.height() as i32;
        self.out_width = out_info.width() as i32;
        self.out_format = cv_image_type_from_video_format(&out_info.format());
        self.out_stride = out_info.stride()[0] as usize;
        self.out_info = Some(out_info);
    }

    fn from_info(in_info: gst_video::VideoInfo, out_info: gst_video::VideoInfo) -> Self {
        let in_height = in_info.height() as i32;
        let in_width = in_info.width() as i32;
        let in_stride = in_info.stride()[0] as usize;
        let in_format = cv_image_type_from_video_format(&in_info.format());
        let out_height = out_info.height() as i32;
        let out_width = out_info.width() as i32;
        let out_stride = out_info.stride()[0] as usize;
        let out_format = cv_image_type_from_video_format(&out_info.format());

        Self {
            in_info: Some(in_info),
            out_info: Some(out_info),
            in_height: in_height,
            in_width: in_width,
            in_format: in_format,
            in_stride: in_stride,
            out_height: out_height,
            out_width: out_width,
            out_format: out_format,
            out_stride: out_stride,
            pad_sink_width: 0,
            pad_sink_height: 0,
            mapx: Mat::default().unwrap(),
            mapy: Mat::default().unwrap(),
        }
    }
}

#[derive(Default)]
pub struct Remap {
    settings: Mutex<Settings>,
    state: AtomicRefCell<Mutex<Option<State>>>,
}

static CAT: Lazy<gst::DebugCategory> = Lazy::new(|| {
    gst::DebugCategory::new("remap", gst::DebugColorFlags::empty(), Some("Remap filter"))
});

#[glib::object_subclass]
impl ObjectSubclass for Remap {
    const NAME: &'static str = "Remap";
    type Type = super::Remap;
    type ParentType = gst_base::BaseTransform;
    type Instance = gst::subclass::ElementInstanceStruct<Self>;
}

impl ObjectImpl for Remap {
    fn properties() -> &'static [glib::ParamSpec] {
        static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
            vec![
                glib::ParamSpec::string(
                    "mapx",
                    "Path to mapx tiff file",
                    "Mapx in tiff",
                    None,
                    glib::ParamFlags::READWRITE | gst::PARAM_FLAG_MUTABLE_READY,
                ),
                glib::ParamSpec::string(
                    "mapy",
                    "Path to mapx tiff file",
                    "Mapy in tiff",
                    None,
                    glib::ParamFlags::READWRITE | gst::PARAM_FLAG_MUTABLE_READY,
                ),
            ]
        });

        PROPERTIES.as_ref()
    }

    fn set_property(
        &self,
        obj: &Self::Type,
        _id: usize,
        value: &glib::Value,
        pspec: &glib::ParamSpec,
    ) {
        let mut settings = self.settings.lock().unwrap();
        match pspec.get_name() {
            "mapx" => {
                let mapx: String = value.get::<String>().unwrap().unwrap();
                settings.mapx = mapx.clone();
            }
            "mapy" => {
                let mapy: String = value.get::<String>().unwrap().unwrap();
                settings.mapy = mapy.clone();
            }
            _ => unimplemented!(),
        }

        if settings.has_maps() {}
    }

    fn get_property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
        match pspec.get_name() {
            "mapx" => {
                let settings = self.settings.lock().unwrap();
                settings.mapx.to_value()
            }
            "mapy" => {
                let settings = self.settings.lock().unwrap();
                settings.mapy.to_value()
            }
            _ => unimplemented!(),
        }
    }
}

fn gst_caps_from_cv_image_type(cv_type: i32) -> Vec<gst_video::VideoFormat> {
    let mut ret = Vec::new();
    match cv_type {
        core::CV_8UC1 => ret.push(gst_video::VideoFormat::Gray8),
        core::CV_8UC3 => {
            ret.push(gst_video::VideoFormat::Rgb);
            ret.push(gst_video::VideoFormat::Bgr);
        }
        core::CV_8UC4 => {
            ret.push(gst_video::VideoFormat::Rgbx);
            ret.push(gst_video::VideoFormat::Xrgb);
            ret.push(gst_video::VideoFormat::Bgrx);
            ret.push(gst_video::VideoFormat::Xbgr);
            ret.push(gst_video::VideoFormat::Rgba);
            ret.push(gst_video::VideoFormat::Argb);
            ret.push(gst_video::VideoFormat::Bgra);
            ret.push(gst_video::VideoFormat::Abgr);
        }
        core::CV_16UC1 => {
            ret.push(gst_video::VideoFormat::Gray16Le);
            ret.push(gst_video::VideoFormat::Gray16Be);
        }
        _ => unimplemented!(),
    }
    ret
}

impl ElementImpl for Remap {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static ELEMENT_METADATA: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
            gst::subclass::ElementMetadata::new(
                "Remap",
                "Filter/Effect/Video",
                "Remaps image using opencv",
                "Vladislav Bortnikov <bortnikov@rerotor.ru>",
            )
        });

        Some(&*ELEMENT_METADATA)
    }

    fn pad_templates() -> &'static [gst::PadTemplate] {
        static PAD_TEMPLATES: Lazy<Vec<gst::PadTemplate>> = Lazy::new(|| {
            // src pad capabilities
            let mut formats: Vec<gst_video::VideoFormat> = Vec::new();
            formats.extend(gst_caps_from_cv_image_type(core::CV_8UC1));
            formats.extend(gst_caps_from_cv_image_type(core::CV_8UC3));
            formats.extend(gst_caps_from_cv_image_type(core::CV_8UC4));
            formats.extend(gst_caps_from_cv_image_type(core::CV_16UC1));
            let str_formats = formats.iter().map(|f| f.to_str().to_send_value()).collect();
            let caps = gst::Caps::new_simple(
                "video/x-raw",
                &[
                    ("format", &gst::List::from_owned(str_formats)),
                    ("width", &gst::IntRange::<i32>::new(0, i32::MAX)),
                    ("height", &gst::IntRange::<i32>::new(0, i32::MAX)),
                    (
                        "framerate",
                        &gst::FractionRange::new(
                            gst::Fraction::new(0, 1),
                            gst::Fraction::new(i32::MAX, 1),
                        ),
                    ),
                ],
            );

            let src_pad_template = gst::PadTemplate::new(
                "src",
                gst::PadDirection::Src,
                gst::PadPresence::Always,
                &caps,
            )
            .unwrap();

            let sink_pad_template = gst::PadTemplate::new(
                "sink",
                gst::PadDirection::Sink,
                gst::PadPresence::Always,
                &caps,
            )
            .unwrap();

            vec![src_pad_template, sink_pad_template]
        });

        PAD_TEMPLATES.as_ref()
    }
}

fn cv_image_type_from_video_format(format: &gst_video::VideoFormat) -> i32 {
    match format {
        gst_video::VideoFormat::Gray8 => core::CV_8UC1,
        gst_video::VideoFormat::Rgb | gst_video::VideoFormat::Bgr => core::CV_8UC3,
        gst_video::VideoFormat::Rgbx
        | gst_video::VideoFormat::Xrgb
        | gst_video::VideoFormat::Bgrx
        | gst_video::VideoFormat::Xbgr
        | gst_video::VideoFormat::Rgba
        | gst_video::VideoFormat::Argb
        | gst_video::VideoFormat::Bgra
        | gst_video::VideoFormat::Abgr => core::CV_8UC4,
        gst_video::VideoFormat::Gray16Le | gst_video::VideoFormat::Gray16Be => core::CV_16UC1,
        _ => unimplemented!(),
    }
}

impl BaseTransformImpl for Remap {
    const MODE: gst_base::subclass::BaseTransformMode =
        gst_base::subclass::BaseTransformMode::NeverInPlace;
    const PASSTHROUGH_ON_SAME_CAPS: bool = false;
    const TRANSFORM_IP_ON_PASSTHROUGH: bool = false;

    fn get_unit_size(&self, _element: &Self::Type, caps: &gst::Caps) -> Option<usize> {
        gst_video::VideoInfo::from_caps(caps)
            .map(|info| info.size())
            .ok()
    }

    fn start(&self, element: &Self::Type) -> Result<(), gst::ErrorMessage> {
        // Drop state
        let mut state = State::default();

        let settings = &self.settings.lock().unwrap();
        let mapx = imgcodecs::imread(settings.mapx.as_str(), imgcodecs::IMREAD_ANYDEPTH)
            .expect("Done reading");
        let mapy = imgcodecs::imread(settings.mapy.as_str(), imgcodecs::IMREAD_ANYDEPTH)
            .expect("Done reading");

        if !mapx.empty().unwrap_or(true) && !mapy.empty().unwrap_or(true) {
            imgproc::convert_maps(
                &mapx,
                &mapy,
                &mut state.mapx,
                &mut state.mapy,
                core::CV_16SC2,
                false,
            )
            .unwrap();
        }
        {
            *self.state.borrow_mut() = Mutex::new(Some(state));
        }
        Ok(())
    }

    fn set_caps(
        &self,
        element: &Self::Type,
        incaps: &gst::Caps,
        outcaps: &gst::Caps,
    ) -> Result<(), gst::LoggableError> {
        let in_info = match gst_video::VideoInfo::from_caps(incaps) {
            Err(_) => return Err(gst::loggable_error!(CAT, "Failed to parse input caps")),
            Ok(info) => info,
        };
        let out_info = match gst_video::VideoInfo::from_caps(outcaps) {
            Err(_) => return Err(gst::loggable_error!(CAT, "Failed to parse output caps")),
            Ok(info) => info,
        };

        gst_info!(
            CAT,
            obj: element,
            "Configured for caps {} to {}",
            incaps,
            outcaps
        );

        let state = self.state.borrow_mut();
        if let Some(state) = state.lock().unwrap().as_mut() {
            state.set_in_info(in_info);
            state.set_out_info(out_info);
        } else {
            unimplemented!();
        }

        Ok(())
    }

    /*fn stop(&self, element: &Self::Type) -> Result<(), gst::ErrorMessage> {
        // Drop state
        gst_info!(CAT, obj: element, "Stopped");
        {
            *self.state.borrow_mut() = Mutex::new(None);
        }

        Ok(())
    }*/

    fn transform(
        &self,
        element: &Self::Type,
        input: &gst::Buffer,
        output: &mut gst::BufferRef,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        let state_guard = self.state.borrow();
        let state_mut = state_guard.lock().unwrap();
        let state = state_mut.as_ref().ok_or(gst::FlowError::NotNegotiated)?;

        let input = gst_video::VideoFrameRef::from_buffer_ref_readable(
            input,
            state.in_info.as_ref().unwrap(),
        )
        .map_err(|_| {
            gst::element_error!(
                element,
                gst::CoreError::Failed,
                ["Failed to map input buffer readable"]
            );
            gst::FlowError::Error
        })?;

        let mut output = gst_video::VideoFrameRef::from_buffer_ref_writable(
            output,
            state.out_info.as_ref().unwrap(),
        )
        .map_err(|_| {
            gst::element_error!(
                element,
                gst::CoreError::Failed,
                ["Failed to map output buffer writable"]
            );
            gst::FlowError::Error
        })?;

        let input_data = input.plane_data(0).unwrap().as_ptr() as *const c_void;
        let output_data = output.plane_data_mut(0).unwrap().as_mut_ptr() as *mut c_void;

        unsafe {
            let input = Mat::new_rows_cols_with_data(
                state.in_height,
                state.in_width,
                state.in_format,
                input_data as *mut c_void,
                state.in_stride,
            )
            .unwrap();
            let mut output = Mat::new_rows_cols_with_data(
                state.out_height,
                state.out_width,
                state.out_format,
                output_data,
                state.out_stride,
            )
            .unwrap();

            imgproc::remap(
                &input,
                &mut output,
                &state.mapx,
                &state.mapy,
                imgproc::INTER_LINEAR,
                core::BORDER_CONSTANT,
                core::Scalar::new(0.0, 0.0, 0.0, 0.0),
            )
            .unwrap();
        }
        Ok(gst::FlowSuccess::Ok)
    }

    fn transform_caps(
        &self,
        element: &Self::Type,
        direction: gst::PadDirection,
        caps: &gst::Caps,
        filter: Option<&gst::Caps>,
    ) -> Option<gst::Caps> {
        let state_guard = self.state.borrow_mut();
        let mut state_mut = state_guard.lock().expect("Got mutex");
        let state = state_mut.as_mut();

        let mut other_caps = caps.clone();
        if let Some(state) = state {
            gst_info!(
                CAT,
                obj: element,
                "MAPX {} {}",
                state.mapx.cols(),
                state.mapx.rows()
            );
            for s in other_caps.make_mut().iter_mut() {
                gst_info!(CAT, obj: element, "S {}", s);
                let in_width = s.get::<i32>("width").unwrap_or(None);
                let in_height = s.get::<i32>("height").unwrap_or(None);
                if let (Some(in_width), Some(in_height)) = (in_width, in_height) {
                    gst_info!(CAT, obj: element, "IN {} {}", in_width, in_height);

                    let out_width = if direction == gst::PadDirection::Sink {
                        state.pad_sink_width = in_width;
                        state.mapx.cols()
                    } else {
                        if state.pad_sink_width > 0 {
                            state.pad_sink_width
                        } else {
                            in_width
                        }
                    };
                    let out_height = if direction == gst::PadDirection::Sink {
                        state.pad_sink_height = in_height;
                        state.mapx.rows()
                    } else {
                        if state.pad_sink_height > 0 {
                            state.pad_sink_height
                        } else {
                            in_height
                        }
                    };
                    gst_info!(
                        CAT,
                        obj: element,
                        "TEST {} {} {} {}",
                        in_width,
                        in_height,
                        out_width,
                        out_height
                    );

                    s.set("width", &out_width);
                    s.set("height", &out_height);
                }
            }
        }
        gst_info!(
            CAT,
            obj: element,
            "Transformed caps from {} to {} in direction {:?}",
            caps,
            other_caps,
            direction
        );
        if let Some(filter) = filter {
            gst_info!(
                CAT,
                obj: element,
                "Filtered caps {} with {} in direction {:?}",
                other_caps,
                filter,
                direction
            );
            Some(filter.intersect_with_mode(&other_caps, gst::CapsIntersectMode::First))
        } else {
            Some(other_caps)
        }
    }
}
