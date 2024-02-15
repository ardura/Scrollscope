use atomic_float::AtomicF32;
use itertools::izip;
use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{
        self,
        plot::{HLine, Line, PlotPoints},
        Color32, Rect, Rounding, Stroke,
    },
    widgets, EguiState,
};
use std::sync::{atomic::Ordering, Arc};
use std::{collections::VecDeque, ops::RangeInclusive, sync::Mutex};

mod slim_checkbox;

/**************************************************
 * Scrollscope by Ardura
 *
 * Build with: cargo xtask bundle scrollscope --profile release
 * Debug with: cargo xtask bundle scrollscope --profile profiling
 * ************************************************/

const ORANGE: Color32 = Color32::from_rgb(239, 123, 69);
const CYAN: Color32 = Color32::from_rgb(14, 177, 210);
const YELLOW: Color32 = Color32::from_rgb(248, 255, 31);
const LIME_GREEN: Color32 = Color32::from_rgb(50, 255, 40);
const MAGENTA: Color32 = Color32::from_rgb(255, 0, 255);
const ELECTRIC_BLUE: Color32 = Color32::from_rgb(0, 153, 255);
const PURPLE: Color32 = Color32::from_rgb(140, 80, 184);
const DARK: Color32 = Color32::from_rgb(40, 40, 40);

#[derive(Enum, Clone, PartialEq)]
pub enum BeatSync {
    Beat,
    Bar,
}

pub struct Scrollscope {
    params: Arc<ScrollscopeParams>,

    // Counter for scaling sample skipping
    skip_counter: i32,
    focused_line_toggle: Arc<Mutex<u8>>,
    is_clipping: Arc<AtomicF32>,
    direction: Arc<Mutex<bool>>,
    enable_main: Arc<Mutex<bool>>,
    enable_aux_1: Arc<Mutex<bool>>,
    enable_aux_2: Arc<Mutex<bool>>,
    enable_aux_3: Arc<Mutex<bool>>,
    enable_aux_4: Arc<Mutex<bool>>,
    enable_aux_5: Arc<Mutex<bool>>,
    enable_sum: Arc<Mutex<bool>>,

    // Data holding values
    samples: Arc<Mutex<VecDeque<f32>>>,
    aux_samples_1: Arc<Mutex<VecDeque<f32>>>,
    aux_samples_2: Arc<Mutex<VecDeque<f32>>>,
    aux_samples_3: Arc<Mutex<VecDeque<f32>>>,
    aux_samples_4: Arc<Mutex<VecDeque<f32>>>,
    aux_samples_5: Arc<Mutex<VecDeque<f32>>>,

    // Syncing for beats
    sync_var: Arc<Mutex<bool>>,
    alt_sync: Arc<Mutex<bool>>,
    in_place_index: Arc<Mutex<i32>>,
    threshold_combo: Arc<Mutex<i32>>,
}

#[derive(Params)]
struct ScrollscopeParams {
    /// The editor state
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    /// Scrollscope scaling for the oscilloscope
    #[id = "free_gain"]
    pub free_gain: FloatParam,

    /// Scrolling speed for GUI
    #[id = "scrollspeed"]
    pub scrollspeed: IntParam,

    /// Horizontal Scaling
    #[id = "scaling"]
    pub h_scale: IntParam,

    /// Sync Timing
    #[id = "Sync Timing"]
    pub sync_timing: EnumParam<BeatSync>,
}

impl Default for Scrollscope {
    fn default() -> Self {
        Self {
            params: Arc::new(ScrollscopeParams::default()),
            skip_counter: 0,
            focused_line_toggle: Arc::new(Mutex::new(0)),
            direction: Arc::new(Mutex::new(false)),
            is_clipping: Arc::new(AtomicF32::new(0.0)),
            enable_main: Arc::new(Mutex::new(true)),
            enable_aux_1: Arc::new(Mutex::new(false)),
            enable_aux_2: Arc::new(Mutex::new(false)),
            enable_aux_3: Arc::new(Mutex::new(false)),
            enable_aux_4: Arc::new(Mutex::new(false)),
            enable_aux_5: Arc::new(Mutex::new(false)),
            enable_sum: Arc::new(Mutex::new(true)),
            samples: Arc::new(Mutex::new(VecDeque::with_capacity(130))),
            aux_samples_1: Arc::new(Mutex::new(VecDeque::with_capacity(130))),
            aux_samples_2: Arc::new(Mutex::new(VecDeque::with_capacity(130))),
            aux_samples_3: Arc::new(Mutex::new(VecDeque::with_capacity(130))),
            aux_samples_4: Arc::new(Mutex::new(VecDeque::with_capacity(130))),
            aux_samples_5: Arc::new(Mutex::new(VecDeque::with_capacity(130))),
            sync_var: Arc::new(Mutex::new(false)),
            alt_sync: Arc::new(Mutex::new(false)),
            in_place_index: Arc::new(Mutex::new(0)),
            threshold_combo: Arc::new(Mutex::new(0)),
        }
    }
}

impl Default for ScrollscopeParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(1040, 520),

            // Input gain dB parameter (free as in unrestricted nums)
            free_gain: FloatParam::new(
                "Input Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-12.0),
                    max: util::db_to_gain(12.0),
                    factor: FloatRange::gain_skew_factor(-12.0, 12.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB Gain")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            // scrollspeed parameter
            scrollspeed: IntParam::new("Length", 100, IntRange::Linear { min: 1, max: 300 })
                .with_unit(" ms"),

            // scaling parameter
            h_scale: IntParam::new("Scale", 24, IntRange::Linear { min: 1, max: 100 })
                .with_unit(" Skip"),

            // Sync timing parameter
            sync_timing: EnumParam::new("Timing", BeatSync::Beat),
        }
    }
}

impl Plugin for Scrollscope {
    const NAME: &'static str = "Scrollscope";
    const VENDOR: &'static str = "Ardura";
    const URL: &'static str = "https://github.com/ardura";
    const EMAIL: &'static str = "azviscarra@gmail.com";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // This looks like it's flexible for running the plugin in mono or stereo
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        //              Inputs                                      Outputs                                 sidechain                               No Idea but needed
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[new_nonzero_u32(2); 5],
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            aux_input_ports: &[new_nonzero_u32(1); 5],
            ..AudioIOLayout::const_default()
        },
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    #[allow(unused_assignments)]
    fn editor(&self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let samples = self.samples.clone();
        let aux_samples_1 = self.aux_samples_1.clone();
        let aux_samples_2 = self.aux_samples_2.clone();
        let aux_samples_3 = self.aux_samples_3.clone();
        let aux_samples_4 = self.aux_samples_4.clone();
        let aux_samples_5 = self.aux_samples_5.clone();
        let ontop = self.focused_line_toggle.clone();
        let is_clipping = self.is_clipping.clone();
        let sync_var = self.sync_var.clone();
        let alt_sync = self.alt_sync.clone();
        let dir_var = self.direction.clone();
        let en_main = self.enable_main.clone();
        let en_aux1 = self.enable_aux_1.clone();
        let en_aux2 = self.enable_aux_2.clone();
        let en_aux3 = self.enable_aux_3.clone();
        let en_aux4 = self.enable_aux_4.clone();
        let en_aux5 = self.enable_aux_5.clone();
        let en_sum = self.enable_sum.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |_, _| {},
            move |egui_ctx, setter, _state| {
                egui::CentralPanel::default().show(egui_ctx, |ui| {
                    // Default colors
                    let mut primary_line_color = ORANGE;
                    let mut aux_line_color = CYAN;
                    let mut aux_line_color_2 = LIME_GREEN;
                    let mut aux_line_color_3 = ELECTRIC_BLUE;
                    let mut aux_line_color_4 = MAGENTA;
                    let mut aux_line_color_5 = PURPLE;
                    let sum_line_color = YELLOW;
                    let background_color = DARK;

                    // Change colors - there's probably a better way to do this
                    let mut style_var = ui.style_mut().clone();
                    style_var.visuals.widgets.inactive.bg_fill = Color32::from_rgb(60, 60, 60);

                    // Assign default colors if user colors not set
                    style_var.visuals.widgets.inactive.fg_stroke.color = primary_line_color;
                    style_var.visuals.widgets.noninteractive.fg_stroke.color = primary_line_color;
                    style_var.visuals.widgets.inactive.bg_stroke.color = primary_line_color;
                    style_var.visuals.widgets.active.fg_stroke.color = primary_line_color;
                    style_var.visuals.widgets.active.bg_stroke.color = primary_line_color;
                    style_var.visuals.widgets.open.fg_stroke.color = primary_line_color;
                    // Param fill
                    style_var.visuals.selection.bg_fill = primary_line_color;

                    style_var.visuals.widgets.noninteractive.bg_stroke.color = aux_line_color;
                    style_var.visuals.widgets.noninteractive.bg_fill = background_color;

                    // Trying to draw background as rect
                    ui.painter()
                        .rect_filled(Rect::EVERYTHING, Rounding::none(), background_color);

                    ui.set_style(style_var);

                    // Reset these to be assigned/reassigned when params change
                    let mut sum_line: Line = Line::new(PlotPoints::default());
                    let mut aux_line: Line = Line::new(PlotPoints::default());
                    let mut aux_line_2: Line = Line::new(PlotPoints::default());
                    let mut aux_line_3: Line = Line::new(PlotPoints::default());
                    let mut aux_line_4: Line = Line::new(PlotPoints::default());
                    let mut aux_line_5: Line = Line::new(PlotPoints::default());
                    let mut line: Line = Line::new(PlotPoints::default());
                    let mut samples = samples.lock().unwrap();
                    let mut aux_samples_1 = aux_samples_1.lock().unwrap();
                    let mut aux_samples_2 = aux_samples_2.lock().unwrap();
                    let mut aux_samples_3 = aux_samples_3.lock().unwrap();
                    let mut aux_samples_4 = aux_samples_4.lock().unwrap();
                    let mut aux_samples_5 = aux_samples_5.lock().unwrap();

                    // The entire "window" container
                    ui.vertical(|ui| {
                        // This is the top bar
                        ui.horizontal(|ui| {
                            ui.label("Scrollscope")
                                .on_hover_text("by Ardura with nih-plug and egui");
                            ui.add(
                                widgets::ParamSlider::for_param(&params.free_gain, setter)
                                    .with_width(30.0),
                            );
                            ui.add_space(4.0);

                            let _scroll_handle = ui.add(
                                widgets::ParamSlider::for_param(&params.scrollspeed, setter)
                                    .with_width(30.0),
                            );

                            ui.add_space(4.0);

                            let _scale_handle = ui.add(
                                widgets::ParamSlider::for_param(&params.h_scale, setter)
                                    .with_width(30.0),
                            );

                            ui.add_space(4.0);
                            let swap_response = ui
                                .button("Toggle Focus")
                                .on_hover_text("Change the drawing order of waveforms");

                            let sync_response = ui
                                .checkbox(&mut sync_var.lock().unwrap(), "Sync")
                                .on_hover_text("Lock drawing to timing");
                            let alt_sync = ui
                                .checkbox(&mut alt_sync.lock().unwrap(), "Alt. Sync")
                                .on_hover_text("Try this if Sync doesn't work");
                            let timing = ui
                                .add(
                                    widgets::ParamSlider::for_param(&params.sync_timing, setter)
                                        .with_width(25.0),
                                )
                                .on_hover_text("Refresh interval when sync enabled");

                            let dir_response = ui
                                .checkbox(&mut dir_var.lock().unwrap(), "Flip")
                                .on_hover_text("Flip direction of oscilloscope");

                            ui.add(slim_checkbox::SlimCheckbox::new(
                                &mut en_main.lock().unwrap(),
                                "In",
                            ));
                            ui.add(slim_checkbox::SlimCheckbox::new(
                                &mut en_aux1.lock().unwrap(),
                                "2",
                            ));
                            ui.add(slim_checkbox::SlimCheckbox::new(
                                &mut en_aux2.lock().unwrap(),
                                "3",
                            ));
                            ui.add(slim_checkbox::SlimCheckbox::new(
                                &mut en_aux3.lock().unwrap(),
                                "4",
                            ));
                            ui.add(slim_checkbox::SlimCheckbox::new(
                                &mut en_aux4.lock().unwrap(),
                                "5",
                            ));
                            ui.add(slim_checkbox::SlimCheckbox::new(
                                &mut en_aux5.lock().unwrap(),
                                "6",
                            ));
                            ui.add(slim_checkbox::SlimCheckbox::new(
                                &mut en_sum.lock().unwrap(),
                                "Sum",
                            ));

                            if swap_response.clicked() {
                                let mut num = ontop.lock().unwrap();
                                // This skips possible "OFF" lines when toggling
                                match *num {
                                    0 => {
                                        if *en_aux1.lock().unwrap() {
                                            *num = 1;
                                        } else if *en_aux2.lock().unwrap() {
                                            *num = 2;
                                        } else if *en_aux3.lock().unwrap() {
                                            *num = 3;
                                        } else if *en_aux4.lock().unwrap() {
                                            *num = 4;
                                        } else if *en_aux5.lock().unwrap() {
                                            *num = 5;
                                        }
                                    }
                                    1 => {
                                        if *en_aux2.lock().unwrap() {
                                            *num = 2;
                                        } else if *en_aux3.lock().unwrap() {
                                            *num = 3;
                                        } else if *en_aux4.lock().unwrap() {
                                            *num = 4;
                                        } else if *en_aux5.lock().unwrap() {
                                            *num = 5;
                                        } else if *en_main.lock().unwrap() {
                                            *num = 0;
                                        }
                                    }
                                    2 => {
                                        if *en_aux3.lock().unwrap() {
                                            *num = 3;
                                        } else if *en_aux4.lock().unwrap() {
                                            *num = 4;
                                        } else if *en_aux5.lock().unwrap() {
                                            *num = 5;
                                        } else if *en_main.lock().unwrap() {
                                            *num = 0;
                                        } else if *en_aux1.lock().unwrap() {
                                            *num = 1;
                                        }
                                    }
                                    3 => {
                                        if *en_aux4.lock().unwrap() {
                                            *num = 4;
                                        } else if *en_aux5.lock().unwrap() {
                                            *num = 5;
                                        } else if *en_main.lock().unwrap() {
                                            *num = 0;
                                        } else if *en_aux1.lock().unwrap() {
                                            *num = 1;
                                        } else if *en_aux2.lock().unwrap() {
                                            *num = 2;
                                        }
                                    }
                                    4 => {
                                        if *en_aux5.lock().unwrap() {
                                            *num = 5;
                                        } else if *en_main.lock().unwrap() {
                                            *num = 0;
                                        } else if *en_aux1.lock().unwrap() {
                                            *num = 1;
                                        } else if *en_aux2.lock().unwrap() {
                                            *num = 2;
                                        } else if *en_aux3.lock().unwrap() {
                                            *num = 3;
                                        }
                                    }
                                    5 => {
                                        if *en_main.lock().unwrap() {
                                            *num = 0;
                                        } else if *en_aux1.lock().unwrap() {
                                            *num = 1;
                                        } else if *en_aux2.lock().unwrap() {
                                            *num = 2;
                                        } else if *en_aux3.lock().unwrap() {
                                            *num = 3;
                                        } else if *en_aux4.lock().unwrap() {
                                            *num = 4;
                                        }
                                    }
                                    _ => {
                                        // Not reachable
                                    }
                                }
                            }
                            // Reset our line on change
                            if sync_response.clicked()
                                || dir_response.clicked()
                                || alt_sync.clicked()
                                || timing.changed()
                            {
                                // Keep same direction when syncing (Issue #12)
                                if sync_response.clicked() {
                                    // If flip selected already, it should be deselected on this click
                                    if *dir_var.lock().unwrap() {
                                        *dir_var.lock().unwrap() = false;
                                    }
                                    // If flip not selected, it should now be selected
                                    else {
                                        *dir_var.lock().unwrap() = true;
                                    }
                                }
                                sum_line = Line::new(PlotPoints::default());
                                aux_line = Line::new(PlotPoints::default());
                                aux_line_2 = Line::new(PlotPoints::default());
                                aux_line_3 = Line::new(PlotPoints::default());
                                aux_line_4 = Line::new(PlotPoints::default());
                                aux_line_5 = Line::new(PlotPoints::default());
                                line = Line::new(PlotPoints::default());
                                samples.clear();
                                aux_samples_1.clear();
                                aux_samples_2.clear();
                                aux_samples_3.clear();
                                aux_samples_4.clear();
                                aux_samples_5.clear();
                            }
                        });
                    });

                    // Reverse our order for drawing if desired (I know this is "slow")
                    if *dir_var.lock().unwrap() {
                        samples.make_contiguous().reverse();
                        aux_samples_1.make_contiguous().reverse();
                        aux_samples_2.make_contiguous().reverse();
                        aux_samples_3.make_contiguous().reverse();
                        aux_samples_4.make_contiguous().reverse();
                        aux_samples_5.make_contiguous().reverse();
                    }

                    ui.allocate_ui(egui::Vec2::new(900.0, 380.0), |ui| {
                        // Fix our colors to focus on our line
                        let lmult: f32 = 0.25;
                        match *ontop.lock().unwrap() {
                            0 => {
                                // Main unaffected
                                aux_line_color = aux_line_color.linear_multiply(lmult);
                                aux_line_color_2 = aux_line_color_2.linear_multiply(lmult);
                                aux_line_color_3 = aux_line_color_3.linear_multiply(lmult);
                                aux_line_color_4 = aux_line_color_4.linear_multiply(lmult);
                                aux_line_color_5 = aux_line_color_5.linear_multiply(lmult);
                            }
                            1 => {
                                // Aux unaffected
                                primary_line_color = primary_line_color.linear_multiply(lmult);
                                aux_line_color_2 = aux_line_color_2.linear_multiply(lmult);
                                aux_line_color_3 = aux_line_color_3.linear_multiply(lmult);
                                aux_line_color_4 = aux_line_color_4.linear_multiply(lmult);
                                aux_line_color_5 = aux_line_color_5.linear_multiply(lmult);
                            }
                            2 => {
                                // Aux 2 unaffected
                                primary_line_color = primary_line_color.linear_multiply(lmult);
                                aux_line_color = aux_line_color.linear_multiply(lmult);
                                aux_line_color_3 = aux_line_color_3.linear_multiply(lmult);
                                aux_line_color_4 = aux_line_color_4.linear_multiply(lmult);
                                aux_line_color_5 = aux_line_color_5.linear_multiply(lmult);
                            }
                            3 => {
                                // Aux 3 unaffected
                                primary_line_color = primary_line_color.linear_multiply(lmult);
                                aux_line_color = aux_line_color.linear_multiply(lmult);
                                aux_line_color_2 = aux_line_color_2.linear_multiply(lmult);
                                aux_line_color_4 = aux_line_color_4.linear_multiply(lmult);
                                aux_line_color_5 = aux_line_color_5.linear_multiply(lmult);
                            }
                            4 => {
                                // Aux 4 unaffected
                                primary_line_color = primary_line_color.linear_multiply(lmult);
                                aux_line_color = aux_line_color.linear_multiply(lmult);
                                aux_line_color_2 = aux_line_color_2.linear_multiply(lmult);
                                aux_line_color_3 = aux_line_color_3.linear_multiply(lmult);
                                aux_line_color_5 = aux_line_color_5.linear_multiply(lmult);
                            }
                            5 => {
                                // Aux 5 unaffected
                                primary_line_color = primary_line_color.linear_multiply(lmult);
                                aux_line_color = aux_line_color.linear_multiply(lmult);
                                aux_line_color_2 = aux_line_color_2.linear_multiply(lmult);
                                aux_line_color_3 = aux_line_color_3.linear_multiply(lmult);
                                aux_line_color_4 = aux_line_color_4.linear_multiply(lmult);
                            }
                            _ => {
                                // We shouldn't be here
                            }
                        }

                        let mut sum_data = samples.clone();

                        // Primary Input
                        let data: PlotPoints = samples
                            .iter()
                            .enumerate()
                            .map(|(i, sample)| {
                                let x: f64;
                                let y: f64;
                                if *en_main.lock().unwrap() {
                                    x = i as f64;
                                    y = *sample as f64;
                                } else {
                                    x = i as f64;
                                    y = 0.0;
                                }
                                [x, y]
                            })
                            .collect();
                        line = Line::new(data)
                            .color(primary_line_color)
                            .stroke(Stroke::new(1.1, primary_line_color));

                        // Aux inputs
                        let aux_data: PlotPoints = aux_samples_1
                            .iter()
                            .enumerate()
                            .map(|(i, sample)| {
                                let x: f64;
                                let y: f64;
                                if *en_aux1.lock().unwrap() {
                                    x = i as f64;
                                    y = *sample as f64;
                                    let sum_temp = sum_data.get_mut(i).unwrap();
                                    *sum_temp += *sample;
                                } else {
                                    x = i as f64;
                                    y = 0.0;
                                }
                                [x, y]
                            })
                            .collect();
                        aux_line = Line::new(aux_data)
                            .color(aux_line_color)
                            .stroke(Stroke::new(1.0, aux_line_color));

                        let aux_data_2: PlotPoints = aux_samples_2
                            .iter()
                            .enumerate()
                            .map(|(i, sample)| {
                                let x: f64;
                                let y: f64;
                                if *en_aux2.lock().unwrap() {
                                    x = i as f64;
                                    y = *sample as f64;
                                    let sum_temp = sum_data.get_mut(i).unwrap();
                                    *sum_temp += *sample;
                                } else {
                                    x = i as f64;
                                    y = 0.0;
                                }
                                [x, y]
                            })
                            .collect();
                        aux_line_2 = Line::new(aux_data_2)
                            .color(aux_line_color_2)
                            .stroke(Stroke::new(1.0, aux_line_color_2));

                        let aux_data_3: PlotPoints = aux_samples_3
                            .iter()
                            .enumerate()
                            .map(|(i, sample)| {
                                let x: f64;
                                let y: f64;
                                if *en_aux3.lock().unwrap() {
                                    x = i as f64;
                                    y = *sample as f64;
                                    let sum_temp = sum_data.get_mut(i).unwrap();
                                    *sum_temp += *sample;
                                } else {
                                    x = i as f64;
                                    y = 0.0;
                                }
                                [x, y]
                            })
                            .collect();
                        aux_line_3 = Line::new(aux_data_3)
                            .color(aux_line_color_3)
                            .stroke(Stroke::new(1.0, aux_line_color_3));

                        let aux_data_4: PlotPoints = aux_samples_4
                            .iter()
                            .enumerate()
                            .map(|(i, sample)| {
                                let x: f64;
                                let y: f64;
                                if *en_aux4.lock().unwrap() {
                                    x = i as f64;
                                    y = *sample as f64;
                                    let sum_temp = sum_data.get_mut(i).unwrap();
                                    *sum_temp += *sample;
                                } else {
                                    x = i as f64;
                                    y = 0.0;
                                }
                                [x, y]
                            })
                            .collect();
                        aux_line_4 = Line::new(aux_data_4)
                            .color(aux_line_color_4)
                            .stroke(Stroke::new(1.0, aux_line_color_4));

                        let aux_data_5: PlotPoints = aux_samples_5
                            .iter()
                            .enumerate()
                            .map(|(i, sample)| {
                                let x: f64;
                                let y: f64;
                                if *en_aux5.lock().unwrap() {
                                    x = i as f64;
                                    y = *sample as f64;
                                    let sum_temp = sum_data.get_mut(i).unwrap();
                                    *sum_temp += *sample;
                                } else {
                                    x = i as f64;
                                    y = 0.0;
                                }
                                [x, y]
                            })
                            .collect();
                        aux_line_5 = Line::new(aux_data_5)
                            .color(aux_line_color_5)
                            .stroke(Stroke::new(1.0, aux_line_color_5));

                        if *en_sum.lock().unwrap() {
                            // Summed audio line
                            let sum_plotpoints: PlotPoints = sum_data
                                .iter()
                                .enumerate()
                                .map(|(i, sample)| {
                                    let x = i as f64;
                                    let y = *sample as f64;
                                    [x, y]
                                })
                                .collect();
                            sum_line = Line::new(sum_plotpoints)
                                .color(sum_line_color.linear_multiply(0.25))
                                .stroke(Stroke::new(0.9, sum_line_color));
                        }

                        egui::plot::Plot::new("Oscilloscope")
                            .show_background(false)
                            .include_x(130.0)
                            .include_y(-1.0)
                            .include_y(1.0)
                            .center_y_axis(true)
                            .allow_zoom(false)
                            .allow_scroll(true)
                            .height(480.0)
                            .width(1040.0)
                            .allow_drag(false)
                            // Blank out the X axis labels
                            .x_axis_formatter(|_, _range: &RangeInclusive<f64>| String::new())
                            // Format hover to blank or value
                            .label_formatter(|_, _| "".to_owned())
                            .show(ui, |plot_ui| {
                                if *en_sum.lock().unwrap() {
                                    // Draw the sum line first so it's furthest behind
                                    plot_ui.line(sum_line);
                                }

                                // Figure out the lines to draw

                                // Draw whichever order next
                                match *ontop.lock().unwrap() {
                                    0 => {
                                        if *en_aux5.lock().unwrap() {
                                            plot_ui.line(aux_line_5);
                                        }
                                        if *en_aux4.lock().unwrap() {
                                            plot_ui.line(aux_line_4);
                                        }
                                        if *en_aux3.lock().unwrap() {
                                            plot_ui.line(aux_line_3);
                                        }
                                        if *en_aux2.lock().unwrap() {
                                            plot_ui.line(aux_line_2);
                                        }
                                        if *en_aux1.lock().unwrap() {
                                            plot_ui.line(aux_line);
                                        }
                                        if *en_main.lock().unwrap() {
                                            plot_ui.line(line);
                                        }
                                    }
                                    1 => {
                                        if *en_main.lock().unwrap() {
                                            plot_ui.line(line);
                                        }
                                        if *en_aux5.lock().unwrap() {
                                            plot_ui.line(aux_line_5);
                                        }
                                        if *en_aux4.lock().unwrap() {
                                            plot_ui.line(aux_line_4);
                                        }
                                        if *en_aux3.lock().unwrap() {
                                            plot_ui.line(aux_line_3);
                                        }
                                        if *en_aux2.lock().unwrap() {
                                            plot_ui.line(aux_line_2);
                                        }
                                        if *en_aux1.lock().unwrap() {
                                            plot_ui.line(aux_line);
                                        }
                                    }
                                    2 => {
                                        if *en_aux1.lock().unwrap() {
                                            plot_ui.line(aux_line);
                                        }
                                        if *en_main.lock().unwrap() {
                                            plot_ui.line(line);
                                        }
                                        if *en_aux5.lock().unwrap() {
                                            plot_ui.line(aux_line_5);
                                        }
                                        if *en_aux4.lock().unwrap() {
                                            plot_ui.line(aux_line_4);
                                        }
                                        if *en_aux3.lock().unwrap() {
                                            plot_ui.line(aux_line_3);
                                        }
                                        if *en_aux2.lock().unwrap() {
                                            plot_ui.line(aux_line_2);
                                        }
                                    }
                                    3 => {
                                        if *en_aux2.lock().unwrap() {
                                            plot_ui.line(aux_line_2);
                                        }
                                        if *en_aux1.lock().unwrap() {
                                            plot_ui.line(aux_line);
                                        }
                                        if *en_main.lock().unwrap() {
                                            plot_ui.line(line);
                                        }
                                        if *en_aux5.lock().unwrap() {
                                            plot_ui.line(aux_line_5);
                                        }
                                        if *en_aux4.lock().unwrap() {
                                            plot_ui.line(aux_line_4);
                                        }
                                        if *en_aux3.lock().unwrap() {
                                            plot_ui.line(aux_line_3);
                                        }
                                    }
                                    4 => {
                                        if *en_aux3.lock().unwrap() {
                                            plot_ui.line(aux_line_3);
                                        }
                                        if *en_aux2.lock().unwrap() {
                                            plot_ui.line(aux_line_2);
                                        }
                                        if *en_aux1.lock().unwrap() {
                                            plot_ui.line(aux_line);
                                        }
                                        if *en_main.lock().unwrap() {
                                            plot_ui.line(line);
                                        }
                                        if *en_aux5.lock().unwrap() {
                                            plot_ui.line(aux_line_5);
                                        }
                                        if *en_aux4.lock().unwrap() {
                                            plot_ui.line(aux_line_4);
                                        }
                                    }
                                    5 => {
                                        if *en_aux4.lock().unwrap() {
                                            plot_ui.line(aux_line_4);
                                        }
                                        if *en_aux3.lock().unwrap() {
                                            plot_ui.line(aux_line_3);
                                        }
                                        if *en_aux2.lock().unwrap() {
                                            plot_ui.line(aux_line_2);
                                        }
                                        if *en_aux1.lock().unwrap() {
                                            plot_ui.line(aux_line);
                                        }
                                        if *en_main.lock().unwrap() {
                                            plot_ui.line(line);
                                        }
                                        if *en_aux5.lock().unwrap() {
                                            plot_ui.line(aux_line_5);
                                        }
                                    }
                                    _ => {
                                        // We shouldn't be here
                                    }
                                }

                                // Draw our clipping guides if needed
                                let clip_counter = is_clipping.load(Ordering::Relaxed);
                                if clip_counter > 0.0 {
                                    plot_ui.hline(
                                        HLine::new(1.0)
                                            .color(Color32::RED)
                                            .stroke(Stroke::new(0.6, Color32::RED)),
                                    );
                                    plot_ui.hline(
                                        HLine::new(-1.0)
                                            .color(Color32::RED)
                                            .stroke(Stroke::new(0.6, Color32::RED)),
                                    );
                                    is_clipping.store(clip_counter - 1.0, Ordering::Relaxed);
                                }
                            })
                            .response;
                    });

                    // Put things back after drawing so process() isn't broken
                    if *dir_var.lock().unwrap() {
                        samples.make_contiguous().reverse();
                        aux_samples_1.make_contiguous().reverse();
                        aux_samples_2.make_contiguous().reverse();
                        aux_samples_3.make_contiguous().reverse();
                        aux_samples_4.make_contiguous().reverse();
                        aux_samples_5.make_contiguous().reverse();
                    }
                });
            },
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        _buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        true
    }

    fn process(
        &mut self,
        buffer: &mut nih_plug::prelude::Buffer<'_>,
        aux: &mut nih_plug::prelude::AuxiliaryBuffers<'_>,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Only process if the GUI is open
        if self.params.editor_state.is_open() {
            let sample_rate: f32 = context.transport().sample_rate;
            // Reset this every buffer process
            self.skip_counter = 0;

            // Get iterators outside the loop
            // These are immutable to not break borrows and the .to_iter() things that return borrows
            let aux_0 = aux.inputs[0].as_slice_immutable();
            let aux_1 = aux.inputs[1].as_slice_immutable();
            let aux_2 = aux.inputs[2].as_slice_immutable();
            let aux_3 = aux.inputs[3].as_slice_immutable();
            let aux_4 = aux.inputs[4].as_slice_immutable();

            for (b0, ax0, ax1, ax2, ax3, ax4) in
                izip!(buffer.iter_samples(), aux_0, aux_1, aux_2, aux_3, aux_4)
            {
                // Beat syncing control
                if *self.sync_var.lock().unwrap() {
                    // Make the current bar precision a one thousandth of a beat - I couldn't find a better way to do this
                    let mut current_beat: f64 = context.transport().pos_beats().unwrap();
                    if *self.alt_sync.lock().unwrap() {
                        // Jitter reduction in timeline dependent DAWs
                        let start_pos = context.transport().bar_start_pos_beats().unwrap();
                        if current_beat < start_pos {
                            continue;
                        }
                        // This should still play well with other DAWs using this timing
                        current_beat =
                            ((0.075 + current_beat) * 10000.0 as f64).ceil() / 10000.0 as f64;
                        // I found this through trial and error w/ Ardour on Windows
                        let threshold = 0.045759;
                        // Added in Issue #11 Alternate timing for other DAWs
                        match self.params.sync_timing.value() {
                            BeatSync::Bar => {
                                // Tracks based off beat number for other daws - this is a mutex instead of atomic for locking
                                if current_beat % 4.0 <= threshold {
                                    // If this is the first time we've been under our threshold, reset our drawing
                                    if *self.threshold_combo.lock().unwrap() == 0 {
                                        // I'm wondering if this reassign was part of the jitter issue instead of being an update so I changed that too
                                        *self.in_place_index.lock().unwrap() = 0;
                                        //self.skip_counter = 0;
                                    }
                                    // Increment here so multiple threshold hits in a row don't stack
                                    *self.threshold_combo.lock().unwrap() += 1;
                                } else {
                                    // We haven't met threshold, keep it 0
                                    *self.threshold_combo.lock().unwrap() = 0;
                                }
                            }
                            BeatSync::Beat => {
                                // Tracks based off beat number for other daws - this is a mutex instead of atomic for locking
                                if current_beat % 1.0 <= threshold {
                                    if *self.threshold_combo.lock().unwrap() == 0 {
                                        *self.in_place_index.lock().unwrap() = 0;
                                        //self.skip_counter = 0;
                                    }
                                    *self.threshold_combo.lock().unwrap() += 1;
                                } else {
                                    *self.threshold_combo.lock().unwrap() = 0;
                                }
                            }
                        }
                    } else {
                        // Works in FL Studio but not other daws, hence the previous couple of lines
                        current_beat = (current_beat * 10000.0 as f64).round() / 10000.0 as f64;
                        match self.params.sync_timing.value() {
                            BeatSync::Bar => {
                                if current_beat % 4.0 == 0.0 {
                                    // Reset our index to the sample vecdeques
                                    //self.in_place_index = Arc::new(Mutex::new(0));
                                    *self.in_place_index.lock().unwrap() = 0;
                                    self.skip_counter = 0;
                                }
                            }
                            BeatSync::Beat => {
                                if current_beat % 1.0 == 0.0 {
                                    // Reset our index to the sample vecdeques
                                    //self.in_place_index = Arc::new(Mutex::new(0));
                                    *self.in_place_index.lock().unwrap() = 0;
                                    self.skip_counter = 0;
                                }
                            }
                        }
                    }
                }

                for (
                    sample,
                    aux_sample_1,
                    aux_sample_2,
                    aux_sample_3,
                    aux_sample_4,
                    aux_sample_5,
                ) in izip!(
                    b0,
                    ax0.iter(),
                    ax1.iter(),
                    ax2.iter(),
                    ax3.iter(),
                    ax4.iter()
                ) {
                    // Only grab X(skip_counter) samples to "optimize"
                    if self.skip_counter % self.params.h_scale.value() == 0 {
                        let current_gain = self.params.free_gain.smoothed.next();
                        // Apply gain to main signal
                        let visual_main_sample: f32 = *sample * current_gain;
                        // Apply gain to sidechains if it isn't doubled up/cloned (FL Studio does this)
                        let visual_aux_sample_1 = if *aux_sample_1 != *sample {
                            *aux_sample_1 * current_gain
                        } else {
                            0.0
                        };
                        let visual_aux_sample_2 = if *aux_sample_2 != *sample {
                            *aux_sample_2 * current_gain
                        } else {
                            0.0
                        };
                        let visual_aux_sample_3 = if *aux_sample_3 != *sample {
                            *aux_sample_3 * current_gain
                        } else {
                            0.0
                        };
                        let visual_aux_sample_4 = if *aux_sample_4 != *sample {
                            *aux_sample_4 * current_gain
                        } else {
                            0.0
                        };
                        let visual_aux_sample_5 = if *aux_sample_5 != *sample {
                            *aux_sample_5 * current_gain
                        } else {
                            0.0
                        };

                        // Set clipping flag if absolute gain over 1
                        if visual_main_sample.abs() > 1.0
                            || visual_aux_sample_1.abs() > 1.0
                            || visual_aux_sample_2.abs() > 1.0
                            || visual_aux_sample_3.abs() > 1.0
                            || visual_aux_sample_4.abs() > 1.0
                            || visual_aux_sample_5.abs() > 1.0
                        {
                            self.is_clipping.store(120.0, Ordering::Relaxed);
                        }
                        // Update our main samples vector for oscilloscope drawing
                        let mut guard = self.samples.lock().unwrap();
                        // Update our sidechain samples vector for oscilloscope drawing
                        let mut aux_guard = self.aux_samples_1.lock().unwrap();
                        let mut aux_guard_2 = self.aux_samples_2.lock().unwrap();
                        let mut aux_guard_3 = self.aux_samples_3.lock().unwrap();
                        let mut aux_guard_4 = self.aux_samples_4.lock().unwrap();
                        let mut aux_guard_5 = self.aux_samples_5.lock().unwrap();
                        // If beat sync is on, we need to process changes in place
                        if *self.sync_var.lock().unwrap() {
                            // Access the in place index
                            let ipi_index: usize = *self.in_place_index.lock().unwrap() as usize;
                            // Check if our indexes exists
                            let main_element: Option<&f32> = guard.get(ipi_index);
                            let aux_element: Option<&f32> = aux_guard.get(ipi_index);
                            let aux_element_2: Option<&f32> = aux_guard_2.get(ipi_index);
                            let aux_element_3: Option<&f32> = aux_guard_3.get(ipi_index);
                            let aux_element_4: Option<&f32> = aux_guard_4.get(ipi_index);
                            let aux_element_5: Option<&f32> = aux_guard_5.get(ipi_index);
                            if main_element.is_some() {
                                // Modify our index since it exists (this compensates for scale/sample changes)
                                let main_index_value: &mut f32 = guard.get_mut(ipi_index).unwrap();
                                *main_index_value = visual_main_sample;
                            }
                            if aux_element.is_some() {
                                // Modify our index since it exists (this compensates for scale/sample changes)
                                let aux_index_value: &mut f32 =
                                    aux_guard.get_mut(ipi_index).unwrap();
                                *aux_index_value = visual_aux_sample_1;
                            }
                            if aux_element_2.is_some() {
                                // Modify our index since it exists (this compensates for scale/sample changes)
                                let aux_index_value_2: &mut f32 =
                                    aux_guard_2.get_mut(ipi_index).unwrap();
                                *aux_index_value_2 = visual_aux_sample_2;
                            }
                            if aux_element_3.is_some() {
                                // Modify our index since it exists (this compensates for scale/sample changes)
                                let aux_index_value_3: &mut f32 =
                                    aux_guard_3.get_mut(ipi_index).unwrap();
                                *aux_index_value_3 = visual_aux_sample_3;
                            }
                            if aux_element_4.is_some() {
                                // Modify our index since it exists (this compensates for scale/sample changes)
                                let aux_index_value_4: &mut f32 =
                                    aux_guard_4.get_mut(ipi_index).unwrap();
                                *aux_index_value_4 = visual_aux_sample_4;
                            }
                            if aux_element_5.is_some() {
                                // Modify our index since it exists (this compensates for scale/sample changes)
                                let aux_index_value_5: &mut f32 =
                                    aux_guard_5.get_mut(ipi_index).unwrap();
                                *aux_index_value_5 = visual_aux_sample_5;
                            }
                            // Increment our in_place_index now that we have substituted
                            *self.in_place_index.lock().unwrap() = ipi_index as i32 + 1;
                        }
                        // Beat sync is off: allow "scroll"
                        else {
                            guard.push_front(visual_main_sample);
                            aux_guard.push_front(visual_aux_sample_1);
                            aux_guard_2.push_front(visual_aux_sample_2);
                            aux_guard_3.push_front(visual_aux_sample_3);
                            aux_guard_4.push_front(visual_aux_sample_4);
                            aux_guard_5.push_front(visual_aux_sample_5);
                        }
                        // ms = samples/samplerate so ms*samplerate = samples
                        // Limit the size of the vecdeques to X elements
                        let scroll: usize = (sample_rate as usize / 1000.0 as usize)
                            * self.params.scrollspeed.value() as usize;
                        if guard.len() != scroll {
                            guard.resize(scroll, 0.0);
                        }
                        if aux_guard.len() != scroll {
                            aux_guard.resize(scroll, 0.0);
                        }
                        if aux_guard_2.len() != scroll {
                            aux_guard_2.resize(scroll, 0.0);
                        }
                        if aux_guard_3.len() != scroll {
                            aux_guard_3.resize(scroll, 0.0);
                        }
                        if aux_guard_4.len() != scroll {
                            aux_guard_4.resize(scroll, 0.0);
                        }
                        if aux_guard_5.len() != scroll {
                            aux_guard_5.resize(scroll, 0.0);
                        }
                    }
                }
                self.skip_counter += 1;
            }
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for Scrollscope {
    const CLAP_ID: &'static str = "com.ardura.scrollscope";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A simple scrolling oscilloscope");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for Scrollscope {
    const VST3_CLASS_ID: [u8; 16] = *b"ScrollscopeAAAAA";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Analyzer];
}

nih_export_clap!(Scrollscope);
nih_export_vst3!(Scrollscope);
