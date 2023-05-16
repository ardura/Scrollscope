//use crossbeam::atomic::AtomicCell;
use atomic_float::AtomicF32;
use nih_plug::prelude::{util, Editor};
use nih_plug_vizia::vizia::vg::Color;
use nih_plug_vizia::vizia::{prelude::*, vg};
use nih_plug_vizia::widgets::*;
use nih_plug_vizia::{assets, create_vizia_editor, ViziaState, ViziaTheming};
use crate::Oscilloscope;
use std::sync::atomic::{Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::GainParams;

#[derive(Clone, Lens)]
struct Data {
    /// Determines which parts of the GUI are visible, and in turn decides the GUI's size.
    params: Arc<GainParams>,
    in_meter: Arc<AtomicF32>,
}

impl Model for Data {}

// Makes sense to also define this here, makes it a bit easier to keep track of
pub(crate) fn default_state() -> Arc<ViziaState> {
    ViziaState::new(|| (800, 320))
}

pub(crate) fn create(
    params: Arc<GainParams>,
    in_meter: Arc<AtomicF32>,
    editor_state: Arc<ViziaState>,
    osc_obj: Arc<Oscilloscope>,
) -> Option<Box<dyn Editor>> {
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        assets::register_noto_sans_light(cx);
        //assets::register_noto_sans_thin(cx);

        Data {
            params: params.clone(),
            in_meter: in_meter.clone(),
        }
        .build(cx);

        ResizeHandle::new(cx);

        VStack::new(cx, |cx| {
            HStack::new(cx, |cx| {
                Label::new(cx, "Scrollscope")
                //.font_family(vec![FamilyOwned::Name(String::from(assets::NOTO_SANS_THIN,))])
                .font_size(16.0)
                .height(Pixels(20.0));

                ParamSlider::new(cx, Data::params, |params| &params.free_gain).width(Pixels(700.0));
            });

            PeakMeter::new(cx, Data::in_meter.map(|in_meter| util::gain_to_db(in_meter.load(Ordering::Relaxed))),Some(Duration::from_millis(600)),).min_width(Pixels(780.0));

            Binding::new(cx, osc_obj, |cx, osc_obj| {
                let (xvar,yvar)  = osc_obj.get_scale();
                //cx.fill((0, 0, 0));
                let line_width = 1.0;
                let paint = vg::Paint::color(Color::black()).with_line_width(line_width);
                let mut path = vg::Path::new();
                // Draw the waveform
                for (i, sample) in osc_obj.get_samples().iter().enumerate() {
                    let x = i as f32 * xvar + osc_obj.get_scroll();
                    let y = sample * yvar;

                    path.move_to(x,y,);
                    path.line_to(x ,0.0);
                    
                    // TODO: Figure out how to draw our line for the oscilloscope since this section is wrong
                }
            })


        })
        .row_between(Pixels(0.0));
        //.child_space(Stretch(1.0));
    })
}