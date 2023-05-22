use atomic_float::AtomicF32;
//use ::egui::mutex::Mutex;
use nih_plug::{prelude::*};
use nih_plug_egui::{create_egui_editor, egui::{self, mutex::Mutex}, widgets, EguiState};
use std::{sync::{Arc, atomic::Ordering}};



/**************************************************
 * Scrollscope by Ardura
 * 
 * Build with: cargo xtask bundle scrollscope --profile profiling
 * ************************************************/

/// The time it takes for the peak meter to decay by 12 dB after switching to complete silence.
const PEAK_METER_DECAY_MS: f64 = 100.0;

pub struct Gain {
    params: Arc<GainParams>,

    // normalize the peak meter's response based on the sample rate with this
    peak_meter_decay_weight: f32,

    // Compressor class
    //osc_obj: Oscilloscope,

    // The current data for the different meters
    peak_meter: Arc<AtomicF32>,
    samples: Arc<Mutex<Vec<AtomicF32>>>,
}

#[derive(Params)]
struct GainParams {
    /// The editor state, saved together with the parameter state so the custom scaling can be
    /// restored.
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    /// Gain scaling for the oscilloscope
    #[id = "free_gain"]
    pub free_gain: FloatParam,

    /// Scrolling speed for GUI
    #[id = "scrollspeed"]
    pub scrollspeed: FloatParam,
}

impl Default for Gain {
    fn default() -> Self {
        Self {
            params: Arc::new(GainParams::default()),
            peak_meter_decay_weight: 1.0,
            peak_meter: Arc::new(AtomicF32::new(util::MINUS_INFINITY_DB)),
            samples: Arc::new(Mutex::new(Vec::new())),
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
            scrollspeed: FloatParam::new(
                "Scroll Speed",
                5.0,
                    FloatRange::Linear {min: 0.5, max: 30.0 },
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
        AudioIOLayout {main_input_channels: NonZeroU32::new(2), main_output_channels: NonZeroU32::new(2), ..AudioIOLayout::const_default()},
        AudioIOLayout {main_input_channels: NonZeroU32::new(1), main_output_channels: NonZeroU32::new(1), ..AudioIOLayout::const_default()},
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let peak_meter = self.peak_meter.clone();
        let samples = self.samples.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |_, _| {},
            move |egui_ctx, setter, _state| {
                egui::CentralPanel::default().show(egui_ctx, |ui| {
                    // NOTE: See `plugins/diopser/src/editor.rs` for an example using the generic UI widget

                    ui.horizontal(|ui| {
                        ui.label("Gain");
                        ui.add(widgets::ParamSlider::for_param(&params.free_gain, setter));

                        let peak_meter =
                            util::gain_to_db(peak_meter.load(std::sync::atomic::Ordering::Relaxed));
                        let peak_meter_text = if peak_meter > util::MINUS_INFINITY_DB {
                                format!("{peak_meter:.1} dBFS")
                            } else {
                                String::from("-inf dBFS")
                            };

                            let peak_meter_normalized = (peak_meter + 60.0) / 60.0;
                            ui.allocate_space(egui::Vec2::splat(2.0));
                            ui.add(
                                egui::widgets::ProgressBar::new(peak_meter_normalized)
                                    .text(peak_meter_text),
                            );
                    });
                    
                    ui.horizontal(|ui| {
                        // Oscilloscope code
                        // https://github.com/emilk/egui/blob/master/crates/egui_demo_lib/src/demo/painting.rs?
                        
                        for i in 1..samples.lock().len() {
                            let prev_sample = &samples.lock()[i - 1];
                            let curr_sample = &samples.lock()[i];
                
                            let prev_pos = egui::pos2(i as f32 - 1.0, prev_sample.load(Ordering::Relaxed));
                            let curr_pos = egui::pos2(i as f32, curr_sample.load(Ordering::Relaxed));
                
                            let line_color = egui::Color32::GREEN;
                            let line_stroke = egui::Stroke::new(1.0, line_color);
                            ui.painter().line_segment([prev_pos, curr_pos], line_stroke);
                        }
                    });
                });
            },
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        // After `PEAK_METER_DECAY_MS` milliseconds of pure silence, the peak meter's value should
        // have dropped by 12 dB
        self.peak_meter_decay_weight = 0.25f64.powf((buffer_config.sample_rate as f64 * PEAK_METER_DECAY_MS / 1000.0).recip()) as f32;

        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {

        //widgets::ParamEvent
        // Buffer level
        for channel_samples in buffer.iter_samples() {
            let mut in_amplitude = 0.0;
            let num_samples = channel_samples.len();
            //let scrollspeed = self.params.scrollspeed.value();
            //let samples = &self.samples;

            for sample in channel_samples {
                // Apply gain
                *sample *= self.params.free_gain.smoothed.next();
                
                // Update the input meter amplitude
                in_amplitude += *sample;
                
                // Update our samples vector for oscilloscope
                let mut guard = self.samples.lock();
                guard.push(AtomicF32::new(*sample));

                
                // Limit the size of the vector to 100 elements
                if guard.len() > 100 {
                    guard.remove(0);
                }

            }

            // To save resources, a plugin can (and probably should!) only perform expensive
            // calculations that are only displayed on the GUI while the GUI is open
            if self.params.editor_state.is_open() {
                // Input gain meter
                in_amplitude = (in_amplitude / num_samples as f32).abs();
                let current_peak_meter = self.peak_meter.load(std::sync::atomic::Ordering::Relaxed);
                let new_peak_meter = if in_amplitude > current_peak_meter {
                    in_amplitude
                } else {
                    current_peak_meter * self.peak_meter_decay_weight 
                        + in_amplitude * (1.0 - self.peak_meter_decay_weight)
                };

                self.peak_meter.store(new_peak_meter, std::sync::atomic::Ordering::Relaxed);
            }
        }

        ProcessStatus::Normal
    }

/*
    const MIDI_INPUT: MidiConfig = MidiConfig::None;

    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;

    const HARD_REALTIME_ONLY: bool = false;

    fn task_executor(&self) -> TaskExecutor<Self> {
        // In the default implementation we can simply ignore the value
        Box::new(|_| ())
    }

    fn filter_state(_state: &mut PluginState) {}

    fn reset(&mut self) {}

    fn deactivate(&mut self) {}
    */
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
