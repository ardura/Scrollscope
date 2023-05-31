use atomic_float::AtomicF32;
use nih_plug::{prelude::*};
use nih_plug_egui::{create_egui_editor, egui::{self, mutex::{Mutex}, plot::{Line, PlotPoints, HLine}, Color32, Stroke, Rect, Rounding}, widgets, EguiState};
use std::{sync::{Arc, atomic::{Ordering}}};

/**************************************************
 * Scrollscope by Ardura
 * 
 * Build with: cargo xtask bundle scrollscope --profile profiling
 * ************************************************/

const ORANGE: Color32 = Color32::from_rgb(239,123,69);
const CYAN: Color32 = Color32::from_rgb(14,177,210);
const YELLOW: Color32 = Color32::from_rgb(248, 255, 31);
const DARK: Color32 = Color32::from_rgb(10, 10, 10);

pub struct Gain {
    params: Arc<GainParams>,

    // Counter for scaling sample skipping
    skip_counter: i32,
    toggle_ontop: Arc<Mutex<bool>>,
    is_clipping: Arc<AtomicF32>,
    // TODO: Add aux used and sum used that adjusts based on aux input
    aux_used: Arc<Mutex<bool>>,
    sum_used: Arc<Mutex<bool>>,

    user_color_primary: Arc<Mutex<Color32>>,
    user_color_secondary: Arc<Mutex<Color32>>,
    user_color_sum: Arc<Mutex<Color32>>,
    user_color_background: Arc<Mutex<Color32>>,

    // Data holding values
    samples: Arc<Mutex<Vec<AtomicF32>>>,
    aux_samples: Arc<Mutex<Vec<AtomicF32>>>,

    // Syncing for beats
    sync_var: Arc<Mutex<bool>>,
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
}

impl Default for Gain {
    fn default() -> Self {
        Self {
            params: Arc::new(GainParams::default()),
            skip_counter: 0,
            user_color_primary: Arc::new(Mutex::new(ORANGE)),
            user_color_secondary: Arc::new(Mutex::new(CYAN)),
            user_color_sum: Arc::new(Mutex::new(YELLOW)),
            user_color_background: Arc::new(Mutex::new(DARK)),
            toggle_ontop: Arc::new(Mutex::new(false)),
            aux_used: Arc::new(Mutex::new(false)),
            sum_used: Arc::new(Mutex::new(false)),
            is_clipping: Arc::new(AtomicF32::new(0.0)),
            samples: Arc::new(Mutex::new(Vec::new())),
            aux_samples: Arc::new(Mutex::new(Vec::new())),
            sync_var: Arc::new(Mutex::new(false)),
        }
    }
}

impl Default for GainParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(820, 360),

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
                3500,
                    IntRange::Linear {min: 2000, max: 20000 },
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
        let ontop = self.toggle_ontop.clone();
        let is_clipping = self.is_clipping.clone();
        let sync_var = self.sync_var.clone();
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
                    // I couldn't figure out getting this to update
                    //.frame(Frame::none().fill(*user_color_background.lock()))
                    .show(egui_ctx, |ui| {

                        // Default colors
                        let mut primary_line_color = user_color_primary.lock();
                        let mut aux_line_color = user_color_secondary.lock();
                        let mut sum_line_color = user_color_sum.lock();
                        let mut background_color = user_color_background.lock();

                        // Change colors - there's probably a better way to do this
                        let mut style_var = ui.style_mut().clone();
                        style_var.visuals.widgets.inactive.bg_fill = Color32::from_rgb(34,34,34);

                        // Assign default colors if user colors not set
                        style_var.visuals.widgets.inactive.fg_stroke.color = *primary_line_color;
                        style_var.visuals.widgets.noninteractive.fg_stroke.color = *primary_line_color;
                        style_var.visuals.widgets.inactive.bg_stroke.color = *primary_line_color;
                        style_var.visuals.widgets.active.fg_stroke.color = *primary_line_color;
                        style_var.visuals.widgets.active.bg_stroke.color = *primary_line_color;
                        style_var.visuals.widgets.open.fg_stroke.color = *primary_line_color;
                        // Param fill
                        style_var.visuals.selection.bg_fill = *primary_line_color;

                        style_var.visuals.widgets.noninteractive.bg_stroke.color = *aux_line_color;
                        style_var.visuals.widgets.noninteractive.bg_fill = *background_color;

                        // Trying to draw background as rect
                        ui.painter().rect_filled(Rect::EVERYTHING, Rounding::none(), *background_color);

                        ui.set_style(style_var);

                        ui.vertical(|ui | {
                            ui.horizontal(|ui| {
                                ui.collapsing("Scrollscope",|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("These don't save yet.");
                                        ui.separator();
                                        ui.color_edit_button_srgba(&mut primary_line_color);
                                        ui.color_edit_button_srgba(&mut aux_line_color);
                                        ui.color_edit_button_srgba(&mut sum_line_color);
                                        ui.color_edit_button_srgba(&mut background_color);
                                        ui.add_space(4.0);
                                        ui.label("Programmed by Ardura with nih-plug and egui");
                                    });
                                });

                                ui.label("Gain");
                                ui.add(widgets::ParamSlider::for_param(&params.free_gain, setter).with_width(60.0));

                                ui.add_space(4.0);

                                ui.label("Samples");
                                ui.add(widgets::ParamSlider::for_param(&params.scrollspeed, setter).with_width(60.0));

                                ui.add_space(4.0);

                                ui.label("Skip");
                                ui.add(widgets::ParamSlider::for_param(&params.h_scale, setter).with_width(60.0));

                                ui.add_space(4.0);
                                ui.checkbox(&mut ontop.lock(), "Swap").on_hover_text("Change the drawing order of waveforms");

                                ui.separator();
                                ui.checkbox(&mut sync_var.lock(), "Sync Beat").on_hover_text("Lock drawing to beat");
                            });
                        });
                            

                        ui.allocate_ui(egui::Vec2::splat(100.0), |ui| {
                            let samples = samples.lock();
                            let aux_samples = aux_samples.lock();
                            let mut sum_line = Line::new(PlotPoints::default());
                            let aux_line;// = Line::new(PlotPoints::default());

                            let mut aux_line: Line = Line::new(PlotPoints::default());
                            let mut sum_line: Line = Line::new(PlotPoints::default());

                            // Primary Input
                            let data: PlotPoints = samples
                                .iter()
                                .enumerate()
                                .map(|(i, sample)| {
                                    let x = i as f64;
                                    let y = sample.load(Ordering::Relaxed) as f64;
                                    [x, y]
                                })
                                .collect();
                            let line = Line::new(data).color(*primary_line_color).stroke(Stroke::new(1.2,*primary_line_color));

                            // Aux input
                            let aux_data: PlotPoints = aux_samples
                                .iter()
                                .enumerate()
                                .map(|(i, sample)| {
                                    let x = i as f64;
                                    let y = sample.load(Ordering::Relaxed) as f64;
                                    [x, y]
                                })
                                .collect();
                            aux_line = Line::new(aux_data).color(*aux_line_color).stroke(Stroke::new(1.0,*aux_line_color));
                            
                            // Summed audio line
                            let are_equal = samples.iter().zip(aux_samples.iter()).all(|(a, b)| {
                                a.load(Ordering::SeqCst) == b.load(Ordering::SeqCst)
                            });
                            if !are_equal {
                                let sum_data: PlotPoints = samples
                                .iter()
                                .zip(aux_samples.iter())
                                    .enumerate()
                                    .map(|(a,b)| {
                                        let x: f64 = a as f64;
                                        let y: f64 = (b.0.clone().load(Ordering::Relaxed) + b.1.clone().load(Ordering::Relaxed)).into();
                                        if y > 1.0 || y < -1.0 {
                                            is_clipping.store(120.0, Ordering::Relaxed);
                                        }
                                        [x,y]
                                     }).collect();
                                sum_line = Line::new(sum_data).color(*sum_line_color).stroke(Stroke::new(0.7,*sum_line_color));
                            }

                            egui::plot::Plot::new("Oscilloscope")
                            .show_background(false)
                            .include_x(400.0)
                            .include_y(-1.0)
                            .include_y(1.0)
                            .center_y_axis(true)
                            .allow_zoom(false)
                            .allow_scroll(true)
                            .height(310.0)
                            .width(855.0)
                            .allow_drag(false)
                            .show(ui, |plot_ui| {
                                // Draw the sum line first so it's furthest behind
                                if !are_equal {
                                    plot_ui.line(sum_line);
                                }

                                // Draw whichever order next
                                if *ontop.lock() {
                                    plot_ui.line(line);
                                    if *aux_used.lock()
                                    {
                                        plot_ui.line(aux_line);
                                    }
                                }
                                else {
                                    if *aux_used.lock() {
                                        plot_ui.line(aux_line);
                                    }
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
        context: &mut impl ProcessContext<Self>,
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
                    // Only grab X samples to "optimize"
                    if self.skip_counter % self.params.h_scale.value() == 0 {
                        // Apply gain
                        let visual_sample = *aux_sample * self.params.free_gain.smoothed.next();
                        if visual_sample.abs() > 1.0 {self.is_clipping.store(120.0, Ordering::Relaxed);}

                        // Update our samples vector for oscilloscope
                        let mut aux_guard = self.aux_samples.lock();
                        
                        if *self.sync_var.lock() {
                            aux_guard.insert(0,AtomicF32::new(visual_sample));
                        }
                        else {
                            aux_guard.push(AtomicF32::new(visual_sample));
                        }

                        // Limit the size of the vector to X elements
                        let scroll = self.params.scrollspeed.value() as usize;
                        if aux_guard.len() > scroll {
                            let trim_amount = aux_guard.len() - scroll;
                            if *self.sync_var.lock() {
                                aux_guard.truncate(trim_amount);
                            }
                            else {
                                aux_guard.drain(0..=trim_amount);
                            }
                        }
                        self.skip_counter += 1;
                    }
                }
            }

            // Reset this every buffer process
            self.skip_counter = 0;

            // Process the main audio
            for channel_samples in buffer.iter_samples() {
                for sample in channel_samples {
                    if *self.sync_var.lock() {
                        let mut current_bar_position: f32 = context.transport().pos_beats().unwrap() as f32;
                        current_bar_position = (current_bar_position * 100.0).round() / 100.0;
                        if  current_bar_position % 1.0 == 0.0 {
                            self.samples.lock().iter_mut().map(|x| *x = AtomicF32::new(0.0)).count();
                            self.aux_samples.lock().iter_mut().map(|x| *x = AtomicF32::new(0.0)).count();
                        }
                    }

                    // Only grab X samples to "optimize"
                    if self.skip_counter % self.params.h_scale.value() == 0 {
                        // Apply gain
                        let visual_sample2 = *sample * self.params.free_gain.smoothed.next();
                        if visual_sample2.abs() > 1.0 {self.is_clipping.store(120.0, Ordering::Relaxed);}
                    
                        // Update our samples vector for oscilloscope
                        let mut guard = self.samples.lock();
                        guard.push(AtomicF32::new(visual_sample2));
                        
                        // Limit the size of the vector to X elements
                        let scroll = self.params.scrollspeed.value() as usize;
                        if guard.len() > scroll {
                            let trim_amount = guard.len() - scroll;
                            if *self.sync_var.lock() {
                                guard.truncate(trim_amount);
                            }
                            else {
                                guard.drain(0..=trim_amount);
                            }
                        }
                        self.skip_counter += 1;
                    }
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
