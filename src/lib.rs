use atomic_float::AtomicF32;
use nih_plug::{prelude::*};
use nih_plug_egui::{create_egui_editor, egui::{self, mutex::Mutex, plot::{Line, PlotPoints, HLine}, Color32, Frame, RichText, Stroke}, widgets, EguiState};
use std::{sync::{Arc, atomic::Ordering}};

/**************************************************
 * Scrollscope by Ardura
 * 
 * Build with: cargo xtask bundle scrollscope --profile profiling
 * ************************************************/

const ORANGE: Color32 = Color32::from_rgb(239,123,69);
const CYAN: Color32 = Color32::from_rgb(14,177,210);
const YELLOW: Color32 = Color32::from_rgb(248, 255, 31);
const DARK: Color32 = Color32::from_rgb(10, 10, 10);
const GREY: Color32 = Color32::from_rgb(20, 20, 20);

pub struct Gain {
    params: Arc<GainParams>,

    // Counter for scaling sample skipping
    skip_counter: i32,
    
    toggle_ontop: Arc<Mutex<bool>>,

    is_clipping: Arc<AtomicF32>,

    user_color_primary: Color32,
    user_color_secondary: Color32,
    user_color_sum: Color32,
    user_color_background: Color32,

    // Data holding values
    samples: Arc<Mutex<Vec<AtomicF32>>>,
    aux_samples: Arc<Mutex<Vec<AtomicF32>>>,
    sum_samples: Arc<Mutex<Vec<AtomicF32>>>,
}

#[derive(Params)]
struct GainParams {
    /// The editor state
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    /// Gain scaling for the oscilloscope
    #[id = "free_gain"]
    pub free_gain: FloatParam,

    /// Scrolling speed for GUI
    #[id = "scrollspeed"]
    pub scrollspeed: IntParam,

    /// Horizontal Scaling
    #[id = "scaling"]
    pub h_scale: IntParam,

    //#[id = "toggle_ontop"]
    //pub toggle_ontop: BoolParam,
}

impl Default for Gain {
    fn default() -> Self {
        Self {
            params: Arc::new(GainParams::default()),
            skip_counter: 0,
            user_color_primary: ORANGE,
            user_color_secondary: CYAN,
            user_color_sum: YELLOW,
            user_color_background: DARK,
            toggle_ontop: Arc::new(Mutex::new(false)),
            is_clipping: Arc::new(AtomicF32::new(0.0)),
            samples: Arc::new(Mutex::new(Vec::new())),
            aux_samples: Arc::new(Mutex::new(Vec::new())),
            sum_samples: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Default for GainParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(800, 320),

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
            .with_unit(" Input Gain")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            // scrollspeed parameter
            scrollspeed: IntParam::new(
                "Samples",
                8000,
                    IntRange::Linear {min: 5000, max: 25000 },
            ),

            // scaling parameter
            h_scale: IntParam::new(
                "Scale",
                20,
                    IntRange::Linear {min: 1, max: 50 },
            ),
        }
    }
}

impl Plugin for Gain {
    const NAME: &'static str = "Scrollscope";
    const VENDOR: &'static str = "Ardura";
    const URL: &'static str = "https://github.com/ardura";
    const EMAIL: &'static str = "azviscarra@gmail.com";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // This looks like it's flexible for running the plugin in mono or stereo
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        //              Inputs                                      Outputs                                 sidechain                               No Idea but needed
        AudioIOLayout {main_input_channels: NonZeroU32::new(2), main_output_channels: NonZeroU32::new(2), aux_input_ports: &[new_nonzero_u32(2)], ..AudioIOLayout::const_default()},
        AudioIOLayout {main_input_channels: NonZeroU32::new(1), main_output_channels: NonZeroU32::new(1), aux_input_ports: &[new_nonzero_u32(1)], ..AudioIOLayout::const_default()},
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let samples = self.samples.clone();
        let aux_samples = self.aux_samples.clone();
        let sum_samples = self.sum_samples.clone();
        let ontop = self.toggle_ontop.clone();
        let is_clipping = self.is_clipping.clone();
        let user_color_primary = self.user_color_primary.clone();
        let user_color_secondary = self.user_color_secondary.clone();
        let user_color_sum = self.user_color_sum.clone();
        let user_color_background = self.user_color_background.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |_, _| {},
            move |egui_ctx, setter, _state| {
                egui::CentralPanel::default()
                    .frame(Frame::none().fill(Color32::from_rgb(10,10,10)))
                    .show(egui_ctx, |ui| {

                        // Default colors
                        let primay_line_color: Color32 = user_color_primary;
                        let aux_line_color: Color32 = user_color_secondary;
                        let sum_line_color: Color32 = user_color_sum;

                        // Change colors - there's probably a better way to do this
                        let mut style_var = ui.style_mut().clone();
                        style_var.visuals.widgets.inactive.bg_fill = GREY;

                        // Assign default colors if user colors not set
                        style_var.visuals.widgets.inactive.fg_stroke.color = user_color_primary;
                        style_var.visuals.widgets.noninteractive.fg_stroke.color = user_color_primary;
                        style_var.visuals.widgets.inactive.bg_stroke.color = user_color_primary;
                        style_var.visuals.widgets.active.fg_stroke.color = user_color_primary;
                        style_var.visuals.widgets.active.bg_stroke.color = user_color_primary;
                        style_var.visuals.widgets.open.fg_stroke.color = user_color_primary;
                        // Param fill
                        style_var.visuals.selection.bg_fill = user_color_primary;

                        style_var.visuals.widgets.noninteractive.bg_stroke.color = user_color_secondary;
                        style_var.visuals.widgets.noninteractive.bg_fill = user_color_background;

                        ui.set_style(style_var);

                        ui.horizontal(|ui| {
                            ui.add_space(6.0);
                            ui.label(RichText::new("Scrollscope"));
                            ui.add_space(6.0);

                            ui.label("Gain");
                            ui.add(widgets::ParamSlider::for_param(&params.free_gain, setter).with_width(80.0));

                            ui.add_space(6.0);

                            ui.label("Samples");
                            ui.add(widgets::ParamSlider::for_param(&params.scrollspeed, setter).with_width(80.0));

                            ui.add_space(6.0);

                            ui.label("Scale");
                            ui.add(widgets::ParamSlider::for_param(&params.h_scale, setter).with_width(80.0));

                            ui.add_space(6.0);
                            ui.checkbox(&mut ontop.lock(), "Order").on_hover_text("Change the drawing order of waveforms");
                        });

                        ui.allocate_ui(egui::Vec2::splat(100.0), |ui| {
                            let samples = samples.lock();
                            let aux_samples = aux_samples.lock();
                            let sum_samples = sum_samples.lock();

                            // Primary Input
                            let data: PlotPoints = samples
                                .iter()
                                .enumerate()
                                .map(|(i, sample)| {
                                    //let h_scale = params.h_scale.value() as f64;
                                    //if i as f64 % h_scale == 0.0 {
                                        let x = i as f64;
                                        let y = sample.load(Ordering::Relaxed) as f64;
                                        [x, y]
                                    //} else {
                                    //    None
                                    //}
                                })
                                .collect();
                            let line = Line::new(data).color(primay_line_color);

                            // Aux input
                            let aux_data: PlotPoints = aux_samples
                                .iter()
                                .enumerate()
                                .map(|(i, sample)| {
                                    //let h_scale = params.h_scale.value() as f64;
                                    //if i as f64 % h_scale == 0.0 {
                                        let x = i as f64;
                                        let y = sample.load(Ordering::Relaxed) as f64;
                                        [x, y]
                                    //} else {
                                    //    None
                                    //}
                                })
                                .collect();
                            let aux_line = Line::new(aux_data).color(aux_line_color);

                            // Summed audio line
                            let sum_data: PlotPoints = sum_samples
                                .iter()
                                .enumerate()
                                .map(|(i, sample)| {
                                    //let h_scale = params.h_scale.value() as f64;
                                    //if i as f64 % h_scale == 0.0 {
                                        let x = i as f64;
                                        let y = sample.load(Ordering::Relaxed) as f64;
                                        [x, y]
                                    //} else {
                                    //    None
                                    //}
                                })
                                .collect();
                            let sum_line = Line::new(sum_data).color(sum_line_color);

                            egui::plot::Plot::new("Oscilloscope")
                            .show_background(false)
                            .include_x(400.0)
                            .include_y(-1.0)
                            .include_y(1.0)
                            .center_y_axis(true)
                            .allow_zoom(false)
                            .allow_scroll(true)
                            .height(310.0)
                            .width(835.0)
                            .allow_drag(false)
                            .show(ui, |plot_ui| {
                                // Draw the sum line first so it's furthest behind
                                plot_ui.line(sum_line);
                                // Draw whichever order next
                                if *ontop.lock() {
                                    plot_ui.line(line);
                                    plot_ui.line(aux_line);
                                }
                                else {
                                    plot_ui.line(aux_line);
                                    plot_ui.line(line);
                                }
                                // Draw our clipping guides if needed
                                let clip_counter = is_clipping.load(Ordering::Relaxed);
                                if clip_counter > 0.0 {
                                    plot_ui.hline(HLine::new(1.0).color(Color32::RED).stroke(Stroke::new(3.0, Color32::RED)));
                                    plot_ui.hline(HLine::new(-1.0).color(Color32::RED).stroke(Stroke::new(3.0, Color32::RED)));
                                    is_clipping.store(clip_counter - 1.0, Ordering::Relaxed);
                                }
                            })
                            .response;
                        });
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
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        //widgets::ParamEvent
        // Buffer level

        // Only process if the GUI is open
        if self.params.editor_state.is_open() {

            // Reset this every buffer process
            self.skip_counter = 0;

            // Process the sidechain
            for aux_channel_samples in aux.inputs[0].iter_samples() {
                for aux_sample in aux_channel_samples {
                    // Apply gain
                    let visual_sample = *aux_sample * self.params.free_gain.smoothed.next();
                    if visual_sample.abs() > 1.0 {self.is_clipping.store(120.0, Ordering::Relaxed);}

                    // Only grab X samples to "optimize"
                    if self.skip_counter % self.params.h_scale.value() == 0 {
                        // Update our samples vector for oscilloscope
                        let mut aux_guard = self.aux_samples.lock();
                        aux_guard.push(AtomicF32::new(visual_sample));

                        // Limit the size of the vector to X elements
                        let scroll = self.params.scrollspeed.value() as usize;
                        if aux_guard.len() > scroll {
                            let trim_amount = aux_guard.len() - scroll;
                            aux_guard.drain(0..=trim_amount);
                        }
                    }
                    self.skip_counter += 1;
                }
            }

            

            // Reset this every buffer process
            self.skip_counter = 0;

            // Process the main audio
            for channel_samples in buffer.iter_samples() {
                for sample in channel_samples {
                    // Apply gain
                    let visual_sample2 = *sample * self.params.free_gain.smoothed.next();
                    if visual_sample2.abs() > 1.0 {self.is_clipping.store(120.0, Ordering::Relaxed);}

                    // Only grab X samples to "optimize"
                    if self.skip_counter % self.params.h_scale.value() == 0 {
                        // Update our samples vector for oscilloscope
                        let mut guard = self.samples.lock();
                        guard.push(AtomicF32::new(visual_sample2));

                        // Limit the size of the vector to X elements
                        let scroll = self.params.scrollspeed.value() as usize;
                        if guard.len() > scroll {
                            let trim_amount = guard.len() - scroll;
                            guard.drain(0..=trim_amount);
                        }
                    }
                    self.skip_counter += 1;
                }
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for Gain {
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

impl Vst3Plugin for Gain {
    const VST3_CLASS_ID: [u8; 16] = *b"ScrollscopeAAAAA";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Analyzer];
}

nih_export_clap!(Gain);
nih_export_vst3!(Gain);
