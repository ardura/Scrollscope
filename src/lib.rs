use atomic_float::AtomicF32;
use nih_plug::{prelude::*};
use nih_plug_egui::{create_egui_editor, egui::{self, mutex::{Mutex}, plot::{Line, PlotPoints, HLine}, Color32, Stroke, Rect, Rounding}, widgets, EguiState};
use std::{collections::VecDeque, sync::atomic::AtomicI32, ops::RangeInclusive};
use std::{sync::{Arc, atomic::{Ordering}}};

/**************************************************
 * Scrollscope by Ardura
 * 
 * Build with: cargo xtask bundle scrollscope --profile profiling
 * ************************************************/

const ORANGE: Color32 = Color32::from_rgb(239,123,69);
const CYAN: Color32 = Color32::from_rgb(14,177,210);
const YELLOW: Color32 = Color32::from_rgb(248, 255, 31);
const DARK: Color32 = Color32::from_rgb(40, 40, 40);

pub struct Gain {
    params: Arc<GainParams>,

    // Counter for scaling sample skipping
    skip_counter: i32,
    swap_draw_order: Arc<Mutex<bool>>,
    is_clipping: Arc<AtomicF32>,
    direction: Arc<Mutex<bool>>,

    user_color_primary: Arc<Mutex<Color32>>,
    user_color_secondary: Arc<Mutex<Color32>>,
    user_color_sum: Arc<Mutex<Color32>>,
    user_color_background: Arc<Mutex<Color32>>,

    // Data holding values
    samples: Arc<Mutex<VecDeque<f32>>>,
    aux_samples: Arc<Mutex<VecDeque<f32>>>,

    // Syncing for beats
    sync_var: Arc<Mutex<bool>>,
    alt_sync: Arc<Mutex<bool>>,
    alt_sync_beat: Arc<Mutex<i64>>,
    in_place_index: Arc<AtomicI32>,
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
            swap_draw_order: Arc::new(Mutex::new(false)),
            direction: Arc::new(Mutex::new(false)),
            is_clipping: Arc::new(AtomicF32::new(0.0)),
            samples: Arc::new(Mutex::new(VecDeque::with_capacity(130))),
            aux_samples: Arc::new(Mutex::new(VecDeque::with_capacity(130))),
            sync_var: Arc::new(Mutex::new(false)),
            alt_sync: Arc::new(Mutex::new(false)),
            alt_sync_beat: Arc::new(Mutex::new(0)),
            in_place_index: Arc::new(AtomicI32::new(0)),
        }
    }
}

impl Default for GainParams {
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
            scrollspeed: IntParam::new(
                "Length",
                100,
                    IntRange::Linear {min: 1, max: 800 },
            ).with_unit(" ms"),

            // scaling parameter
            h_scale: IntParam::new(
                "Scale",
                24,
                    IntRange::Linear {min: 1, max: 150 },
            ).with_unit(" Skip"),
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
        let ontop = self.swap_draw_order.clone();
        let is_clipping = self.is_clipping.clone();
        let sync_var = self.sync_var.clone();
        let alt_sync = self.alt_sync.clone();
        let dir_var = self.direction.clone();
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
                    .show(egui_ctx, |ui| {
                        // Default colors
                        let mut primary_line_color = user_color_primary.lock();
                        let mut aux_line_color = user_color_secondary.lock();
                        let mut sum_line_color = user_color_sum.lock();
                        let mut background_color = user_color_background.lock();

                        // Change colors - there's probably a better way to do this
                        let mut style_var = ui.style_mut().clone();
                        style_var.visuals.widgets.inactive.bg_fill = Color32::from_rgb(60,60,60);

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

                        // Reset these to be assigned/reassigned when params change
                        let mut sum_line: Line = Line::new(PlotPoints::default());
                        let mut aux_line: Line = Line::new(PlotPoints::default());
                        let mut line: Line = Line::new(PlotPoints::default());
                        let mut samples: egui::mutex::MutexGuard<VecDeque<f32>> = samples.lock();
                        let mut aux_samples: egui::mutex::MutexGuard<VecDeque<f32>> = aux_samples.lock();

                        ui.vertical(|ui | {
                            ui.horizontal(|ui| {
                                ui.collapsing("Scrollscope",|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("These don't save.");
                                        ui.separator();
                                        ui.color_edit_button_srgba(&mut primary_line_color);
                                        ui.color_edit_button_srgba(&mut aux_line_color);
                                        ui.color_edit_button_srgba(&mut sum_line_color);
                                        ui.color_edit_button_srgba(&mut background_color);
                                        ui.add_space(4.0);
                                        ui.label("by Ardura with nih-plug and egui");
                                    });
                                });

                                let gain_handle = ui.add(widgets::ParamSlider::for_param(&params.free_gain, setter).with_width(60.0));

                                ui.add_space(4.0);

                                let _scroll_handle = ui.add(widgets::ParamSlider::for_param(&params.scrollspeed, setter).with_width(60.0));

                                ui.add_space(4.0);

                                let _scale_handle = ui.add(widgets::ParamSlider::for_param(&params.h_scale, setter).with_width(60.0));

                                ui.add_space(4.0);
                                let _swap_response = ui.checkbox(&mut ontop.lock(), "Swap").on_hover_text("Change the drawing order of waveforms");

                                let sync_response = ui.checkbox(&mut sync_var.lock(), "Sync Beat").on_hover_text("Lock drawing to beat");
                                let alt_sync = ui.checkbox(&mut alt_sync.lock(), "Alt. Sync").on_hover_text("Try this if Sync doesn't work");

                                let dir_response = ui.checkbox(&mut dir_var.lock(), "Flip").on_hover_text("Flip direction of oscilloscope");

                                if gain_handle.changed() {
                                    sum_line = Line::new(PlotPoints::default());
                                }
                                // Reset our line on change
                                if sync_response.clicked() || dir_response.clicked() || alt_sync.clicked()
                                {
                                    sum_line = Line::new(PlotPoints::default());
                                    aux_line = Line::new(PlotPoints::default());
                                    line = Line::new(PlotPoints::default());
                                    samples.clear();
                                    aux_samples.clear();
                                }
                            });
                        });

                        // Reverse our order for drawing if desired (I know this is "slow")
                        if *dir_var.lock() {
                            samples.make_contiguous().reverse();
                            aux_samples.make_contiguous().reverse();
                        }

                        ui.allocate_ui(egui::Vec2::new(900.0,380.0), |ui| {
                            // Primary Input
                            let data: PlotPoints = samples
                                .iter()
                                .enumerate()
                                .map(|(i, sample)| {
                                    let x = i as f64;
                                    let y = *sample as f64;
                                    [x, y]
                                })
                                .collect();
                            line = Line::new(data).color(*primary_line_color).stroke(Stroke::new(1.1,*primary_line_color));

                            // Aux input
                            let aux_data: PlotPoints = aux_samples
                                .iter()
                                .enumerate()
                                .map(|(i, sample)| {
                                    let x = i as f64;
                                    let y = *sample as f64;
                                    [x, y]
                                })
                                .collect();
                            aux_line = Line::new(aux_data).color(*aux_line_color).stroke(Stroke::new(1.0,*aux_line_color));

                            // Summed audio line
                            let sum_data: PlotPoints = samples
                                .iter()
                                .zip(aux_samples.iter())
                                    .enumerate()
                                    .map(|(a,b)| {
                                        let x: f64 = a as f64;
                                        let bval = b.0.clone();
                                        let bval2 = b.1.clone();
                                        let mut y: f64 = 0.0;
                                        if bval != bval2 {
                                            y = (b.0.clone() + b.1.clone()).into();
                                        }
                                        
                                        if y > 1.0 || y < -1.0 {
                                            is_clipping.store(120.0, Ordering::Relaxed);
                                        }
                                        [x,y]
                                     }).collect();
                            sum_line = Line::new(sum_data).color(*sum_line_color).stroke(Stroke::new(0.9,*sum_line_color));
                            
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
                            .x_axis_formatter(|_, _range: &RangeInclusive<f64>| {String::new()})
                            // Format hover to blank or value
                            .label_formatter(|_, _| {"".to_owned()})
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
                                    plot_ui.hline(HLine::new(1.0).color(Color32::RED).stroke(Stroke::new(0.6, Color32::RED)));
                                    plot_ui.hline(HLine::new(-1.0).color(Color32::RED).stroke(Stroke::new(0.6, Color32::RED)));
                                    is_clipping.store(clip_counter - 1.0, Ordering::Relaxed);
                                }
                            })
                            .response;
                        });

                        // Put things back after drawing so process() isn't broken
                        if *dir_var.lock() {
                            samples.make_contiguous().reverse();
                            aux_samples.make_contiguous().reverse();
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
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Only process if the GUI is open
        if self.params.editor_state.is_open() {
            // Reset this every buffer process
            self.skip_counter = 0;

            // Process main channel and sidechain together
            for (mut aux_channel_samples, mut channel_samples) in aux.inputs[0].iter_samples().zip(buffer.iter_samples()) {
                for (aux_sample, sample) in aux_channel_samples.iter_mut().zip(channel_samples.iter_mut()) {
                    
                    // If we are beat syncing - this resets our position in time accordingly
                    if *self.sync_var.lock() {
                        // Make the current bar precision a one thousandth of a beat - I couldn't find a better way to do this
                        let mut current_beat: f64 = context.transport().pos_beats().unwrap();

                        if *self.alt_sync.lock() {
                            let current_bar = current_beat as i64;
                            // Tracks based off beat number for other daws - this is a mutex instead of atomic for locking
                            if *self.alt_sync_beat.lock() != current_bar {
                                self.in_place_index = Arc::new(AtomicI32::new(0));
                                self.skip_counter = 0;
                                *self.alt_sync_beat.lock() = current_bar;
                            }
                        } else {
                            // Works in FL Studio but not other daws, hence the previous couple of lines
                            current_beat = (current_beat * 1000.0 as f64).round() / 1000.0 as f64;
                            if current_beat % 1.0 == 0.0 {
                                // Reset our index to the sample vecdeques
                                self.in_place_index = Arc::new(AtomicI32::new(0));
                                self.skip_counter = 0;
                            }
                        }
                    }

                    // Only grab X(skip_counter) samples to "optimize"
                    if self.skip_counter % self.params.h_scale.value() == 0 {
                        
                        // Apply gain to main signal
                        let visual_main_sample: f32 = *sample * self.params.free_gain.smoothed.next();
                        // Apply gain to sidechain
                        let visual_aux_sample = *aux_sample * self.params.free_gain.smoothed.next();

                        // Set clipping flag if absolute gain over 1
                        if visual_aux_sample.abs() > 1.0 || visual_main_sample.abs() > 1.0 {
                            self.is_clipping.store(120.0, Ordering::Relaxed);
                        }

                        // Update our main samples vector for oscilloscope drawing
                        let mut guard: egui::mutex::MutexGuard<VecDeque<f32>> = self.samples.lock();
                        guard.make_contiguous();

                        // Update our sidechain samples vector for oscilloscope drawing
                        let mut aux_guard: egui::mutex::MutexGuard<VecDeque<f32>> = self.aux_samples.lock();
                        aux_guard.make_contiguous();

                        // If beat sync is on, we need to process changes in place
                        if *self.sync_var.lock() {
                            // Access the Arc - ipi = in place index
                            let ipi: Arc<AtomicI32> = self.in_place_index.clone();
                            let ipi_index: usize = ipi.load(Ordering::Relaxed) as usize;
                            
                            // Check if our indexes exists
                            let main_element: Option<&f32> = guard.get(ipi_index);
                            let aux_element: Option<&f32> = aux_guard.get(ipi_index);

                            if main_element.is_some()
                            {
                                // Modify our index since it exists (this compensates for scale/sample changes)
                                let main_index_value: &mut f32 = guard.get_mut(ipi_index).unwrap();
                                *main_index_value = visual_main_sample;
                            }
                            if aux_element.is_some()
                            {
                                // Modify our index since it exists (this compensates for scale/sample changes)
                                let aux_index_value: &mut f32 = aux_guard.get_mut(ipi_index).unwrap();
                                *aux_index_value = visual_aux_sample;
                            }
                            // Increment our in_place_index now that we have substituted
                            ipi.store((ipi_index + 1).try_into().unwrap(), Ordering::Relaxed);
                        }
                        // Beat sync is off: allow "scroll"
                        else {
                            guard.push_front(visual_main_sample);
                            aux_guard.push_front(visual_aux_sample);
                        }

                        // ms = samples/samplerate so ms*samplerate = samples
                        // Limit the size of the vecdeques to X elements
                        let scroll: usize = (context.transport().sample_rate as usize/1000.0 as usize) * self.params.scrollspeed.value() as usize;
                        if guard.len() != scroll {
                            guard.resize(scroll, 0.0);
                        }
                        if aux_guard.len() != scroll {
                            aux_guard.resize(scroll, 0.0);
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
