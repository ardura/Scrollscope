use atomic_float::{AtomicF32};
use configparser::ini::Ini;
use itertools::{izip};
use nih_plug::{prelude::*};
use nih_plug_egui::{
    create_egui_editor,
    egui::{
        self, epaint, plot::{HLine, Line, PlotPoints}, pos2, Align2, Color32, FontId, Pos2, Rect, Response, Rounding, Stroke
    },
    widgets, EguiState,
};
use rustfft::{num_complex::Complex, FftDirection, FftPlanner};
use std::{env, fs::File, io::Write, path::MAIN_SEPARATOR_STR, str::FromStr, sync::{atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering}, Arc}};
use std::{collections::VecDeque, ops::RangeInclusive, sync::Mutex};

mod slim_checkbox;

/**************************************************
 * Scrollscope v1.3.2 by Ardura
 * "A simple scrolling Oscilloscope has become complex now"
 *
 * Build with: cargo xtask bundle scrollscope --profile release
 * Debug with: cargo xtask bundle scrollscope --profile profiling
 * 
 * If you don't want/need the standalone version you can save time by only compiling the VST + CLAP with "--lib"
 * cargo xtask bundle scrollscope --profile release --lib
 * ************************************************/

#[derive(Enum, Clone, PartialEq)]
pub enum BeatSync {
    Beat,
    Bar,
}

pub struct Scrollscope {
    params: Arc<ScrollscopeParams>,

    // Counter for scaling sample skipping
    skip_counter: i32,
    focused_line_toggle: Arc<AtomicU8>,
    is_clipping: Arc<AtomicF32>,
    direction: Arc<AtomicBool>,
    enable_main: Arc<AtomicBool>,
    enable_aux_1: Arc<AtomicBool>,
    enable_aux_2: Arc<AtomicBool>,
    enable_aux_3: Arc<AtomicBool>,
    enable_aux_4: Arc<AtomicBool>,
    enable_aux_5: Arc<AtomicBool>,
    enable_sum: Arc<AtomicBool>,
    enable_guidelines: Arc<AtomicBool>,
    enable_bar_mode: Arc<AtomicBool>,

    // Data holding values
    samples: Arc<Mutex<VecDeque<f32>>>,
    aux_samples_1: Arc<Mutex<VecDeque<f32>>>,
    aux_samples_2: Arc<Mutex<VecDeque<f32>>>,
    aux_samples_3: Arc<Mutex<VecDeque<f32>>>,
    aux_samples_4: Arc<Mutex<VecDeque<f32>>>,
    aux_samples_5: Arc<Mutex<VecDeque<f32>>>,
    scrolling_beat_lines: Arc<Mutex<VecDeque<f32>>>,

    // Syncing for beats
    sync_var: Arc<AtomicBool>,
    alt_sync: Arc<AtomicBool>,
    in_place_index: Arc<AtomicI32>,
    beat_threshold: Arc<AtomicI32>,
    add_beat_line: Arc<AtomicBool>,

    // FFT/Analyzer
    fft: Arc<Mutex<FftPlanner<f32>>>,
    show_analyzer: Arc<AtomicBool>,

    sample_rate: Arc<AtomicF32>,
    prev_skip: Arc<AtomicI32>,
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
    pub scrollspeed: FloatParam,

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
            focused_line_toggle: Arc::new(AtomicU8::new(0)),
            direction: Arc::new(AtomicBool::new(false)),
            is_clipping: Arc::new(AtomicF32::new(0.0)),
            enable_main: Arc::new(AtomicBool::new(true)),
            enable_aux_1: Arc::new(AtomicBool::new(false)),
            enable_aux_2: Arc::new(AtomicBool::new(false)),
            enable_aux_3: Arc::new(AtomicBool::new(false)),
            enable_aux_4: Arc::new(AtomicBool::new(false)),
            enable_aux_5: Arc::new(AtomicBool::new(false)),
            enable_sum: Arc::new(AtomicBool::new(true)),
            enable_guidelines: Arc::new(AtomicBool::new(true)),
            enable_bar_mode: Arc::new(AtomicBool::new(false)),
            samples: Arc::new(Mutex::new(VecDeque::with_capacity(130))),
            aux_samples_1: Arc::new(Mutex::new(VecDeque::with_capacity(130))),
            aux_samples_2: Arc::new(Mutex::new(VecDeque::with_capacity(130))),
            aux_samples_3: Arc::new(Mutex::new(VecDeque::with_capacity(130))),
            aux_samples_4: Arc::new(Mutex::new(VecDeque::with_capacity(130))),
            aux_samples_5: Arc::new(Mutex::new(VecDeque::with_capacity(130))),
            scrolling_beat_lines: Arc::new(Mutex::new(VecDeque::with_capacity(130))),
            sync_var: Arc::new(AtomicBool::new(false)),
            alt_sync: Arc::new(AtomicBool::new(false)),
            add_beat_line: Arc::new(AtomicBool::new(false)),
            in_place_index: Arc::new(AtomicI32::new(0)),
            beat_threshold: Arc::new(AtomicI32::new(0)),
            fft: Arc::new(Mutex::new(FftPlanner::new())),
            show_analyzer: Arc::new(AtomicBool::new(false)),
            sample_rate: Arc::new(AtomicF32::new(44100.0)),
            prev_skip: Arc::new(AtomicI32::new(24)),
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
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            // scrollspeed parameter
            scrollspeed: FloatParam::new("Length", 100.0, FloatRange::Skewed { min: 1.0, max: 1000.0 , factor: 0.33})
                .with_unit(" ms")
                .with_step_size(1.0),

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

    fn editor(&self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let samples = self.samples.clone();
        let aux_samples_1 = self.aux_samples_1.clone();
        let aux_samples_2 = self.aux_samples_2.clone();
        let aux_samples_3 = self.aux_samples_3.clone();
        let aux_samples_4 = self.aux_samples_4.clone();
        let aux_samples_5 = self.aux_samples_5.clone();
        let scrolling_beat_lines = self.scrolling_beat_lines.clone();
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
        let en_guidelines = self.enable_guidelines.clone();
        let en_bar_mode = self.enable_bar_mode.clone();
        let fft = self.fft.clone();
        let show_analyzer = self.show_analyzer.clone();
        let sample_rate = self.sample_rate.clone();
        let prev_skip = self.prev_skip.clone();
        let mut config = Ini::new();
        let binding = dirs::config_local_dir();
        let location;
        if binding.is_some() {
            location = String::from(binding.unwrap().as_os_str().to_str().unwrap()) + MAIN_SEPARATOR_STR + "Scrollscope.ini";
            let location_clone = location.clone();
            let location_clone_2 = location.clone();
            nih_log!("{}", location);
            let mut _config_loaded = config.load(location);
            if _config_loaded.is_ok() {
                nih_log!("Loaded!");
            } else {
                nih_log!("Not found!");
                let mut file = File::create(location_clone).unwrap();
                // Create our default config file if we can
                let write_result = file.write_all(b"# These are in RGB
[ui_colors]
background = 40,40,40
guidelines = 160,160,160
ui_main_color = 239,123,69
user_main = 239,123,69
user_aux_1 = 14,177,210
user_aux_2 = 50,255,40
user_aux_3 = 0,153,255
user_aux_4 = 255,0,255
user_aux_5 = 230,80,80
user_sum_line = 248,255,31
inactive_bg = 60,60,60");
                if write_result.is_ok() {
                    nih_log!("Created!");
                    _config_loaded = config.load(location_clone_2);
                } else {
                    nih_log!("Coudldn't Create!");
                }
            }
        }
        
        let mut t: Vec<u8> = config.get("ui_colors", "user_main")
            .unwrap()
            .split(',')
            .map(|elem|{
                let u: u8 = FromStr::from_str(elem).unwrap_or_default();
                u
            })
            .collect();
        let primary_line_color = Color32::from_rgb(t[0], t[1], t[2]);
        t = config.get("ui_colors", "background")
            .unwrap()
            .split(',')
            .map(|elem|{
                let u: u8 = FromStr::from_str(elem).unwrap_or_default();
                u
            })
            .collect();
        let background_color = Color32::from_rgb(t[0], t[1], t[2]);
        t = config.get("ui_colors", "guidelines")
            .unwrap()
            .split(',')
            .map(|elem|{
                let u: u8 = FromStr::from_str(elem).unwrap_or_default();
                u
            })
            .collect();
        let guidelines = Color32::from_rgb(t[0], t[1], t[2]);
        t = config.get("ui_colors", "ui_main_color")
            .unwrap()
            .split(',')
            .map(|elem|{
                let u: u8 = FromStr::from_str(elem).unwrap_or_default();
                u
            })
            .collect();
        let ui_main_color = Color32::from_rgb(t[0], t[1], t[2]);
        t = config.get("ui_colors", "user_sum_line")
            .unwrap()
            .split(',')
            .map(|elem|{
                let u: u8 = FromStr::from_str(elem).unwrap_or_default();
                u
            })
            .collect();
        let user_sum_line = Color32::from_rgb(t[0], t[1], t[2]);
        t = config.get("ui_colors", "user_aux_1")
            .unwrap()
            .split(',')
            .map(|elem|{
                let u: u8 = FromStr::from_str(elem).unwrap_or_default();
                u
            })
            .collect();
        let user_aux_1 = Color32::from_rgb(t[0], t[1], t[2]);
        t = config.get("ui_colors", "user_aux_2")
            .unwrap()
            .split(',')
            .map(|elem|{
                let u: u8 = FromStr::from_str(elem).unwrap_or_default();
                u
            })
            .collect();
        let user_aux_2 = Color32::from_rgb(t[0], t[1], t[2]);
        t = config.get("ui_colors", "user_aux_3")
            .unwrap()
            .split(',')
            .map(|elem|{
                let u: u8 = FromStr::from_str(elem).unwrap_or_default();
                u
            })
            .collect();
        let user_aux_3 = Color32::from_rgb(t[0], t[1], t[2]);
        t = config.get("ui_colors", "user_aux_4")
            .unwrap()
            .split(',')
            .map(|elem|{
                let u: u8 = FromStr::from_str(elem).unwrap_or_default();
                u
            })
            .collect();
        let user_aux_4 = Color32::from_rgb(t[0], t[1], t[2]);
        t = config.get("ui_colors", "user_aux_5")
            .unwrap()
            .split(',')
            .map(|elem|{
                let u: u8 = FromStr::from_str(elem).unwrap_or_default();
                u
            })
            .collect();
        let user_aux_5 = Color32::from_rgb(t[0], t[1], t[2]);
        t = config.get("ui_colors", "inactive_bg")
            .unwrap()
            .split(',')
            .map(|elem|{
                let u: u8 = FromStr::from_str(elem).unwrap_or_default();
                u
            })
            .collect();
        let inactive_bg = Color32::from_rgb(t[0], t[1], t[2]);
        
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |_, _| {},
            move |egui_ctx, setter, _state| {
                egui::CentralPanel::default().show(egui_ctx, |ui| {
                    // Change colors - there's probably a better way to do this
                    let style_var = ui.style_mut();
                    style_var.visuals.widgets.inactive.bg_fill = inactive_bg;

                    // Assign default colors if user colors not set
                    style_var.visuals.widgets.inactive.fg_stroke.color = ui_main_color;
                    style_var.visuals.widgets.noninteractive.fg_stroke.color = primary_line_color;
                    style_var.visuals.widgets.inactive.bg_stroke.color = primary_line_color;
                    style_var.visuals.widgets.active.fg_stroke.color = ui_main_color;
                    style_var.visuals.widgets.active.bg_stroke.color = primary_line_color;
                    style_var.visuals.widgets.open.fg_stroke.color = primary_line_color;
                    // Param fill
                    style_var.visuals.selection.bg_fill = primary_line_color;

                    style_var.visuals.widgets.noninteractive.bg_stroke.color = guidelines;
                    style_var.visuals.widgets.noninteractive.bg_fill = background_color;

                    // Trying to draw background as rect
                    ui.painter()
                        .rect_filled(Rect::EVERYTHING, Rounding::none(), background_color);

                    //ui.set_style(style_var);

                    // Reset these to be assigned/reassigned when params change
                    let mut sum_line: Line = Line::new(PlotPoints::default());
                    let mut aux_line: Line = Line::new(PlotPoints::default());
                    let mut aux_line_2: Line = Line::new(PlotPoints::default());
                    let mut aux_line_3: Line = Line::new(PlotPoints::default());
                    let mut aux_line_4: Line = Line::new(PlotPoints::default());
                    let mut aux_line_5: Line = Line::new(PlotPoints::default());
                    let mut scrolling_beat_line: Line = Line::new(PlotPoints::default());
                    let mut line: Line = Line::new(PlotPoints::default());
                    let mut samples = samples.lock().unwrap();
                    let mut aux_samples_1 = aux_samples_1.lock().unwrap();
                    let mut aux_samples_2 = aux_samples_2.lock().unwrap();
                    let mut aux_samples_3 = aux_samples_3.lock().unwrap();
                    let mut aux_samples_4 = aux_samples_4.lock().unwrap();
                    let mut aux_samples_5 = aux_samples_5.lock().unwrap();
                    let mut scrolling_beat_lines = scrolling_beat_lines.lock().unwrap();
                    let sr = sample_rate.clone();

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

                            let swap_response: Response;
                            if show_analyzer.load(Ordering::SeqCst) {
                                let _scroll_handle = ui.add(
                                    widgets::ParamSlider::for_param(&params.scrollspeed, setter)
                                        .with_width(120.0),
                                );

                                ui.add_space(4.0);
                                
                                swap_response = ui
                                    .button("Toggle Focus")
                                    .on_hover_text("Change the drawing order of waveforms");
                            } else {
                                let _scroll_handle = ui.add(
                                    widgets::ParamSlider::for_param(&params.scrollspeed, setter)
                                        .with_width(50.0),
                                );
    
                                ui.add_space(4.0);
    
                                let _scale_handle = ui.add(
                                    widgets::ParamSlider::for_param(&params.h_scale, setter)
                                        .with_width(30.0),
                                );
                                ui.add_space(4.0);
                                swap_response = ui
                                    .button("Toggle Focus")
                                    .on_hover_text("Change the drawing order of waveforms");

                                let sync_box = slim_checkbox::AtomicSlimCheckbox::new(&sync_var, "Sync");
                                let sync_response = ui.add(sync_box).on_hover_text("Lock drawing to timing");
                                let alt_sync_box = slim_checkbox::AtomicSlimCheckbox::new(&alt_sync, "Alt Sync");
                                let alt_sync_response = ui.add(alt_sync_box).on_hover_text("Try this if Sync doesn't work");

                                let timing_response = ui
                                    .add(
                                        widgets::ParamSlider::for_param(&params.sync_timing, setter)
                                            .with_width(25.0),
                                    )
                                    .on_hover_text("Refresh interval when sync enabled");

                                let dir_box = slim_checkbox::AtomicSlimCheckbox::new(&dir_var, "Flip");
                                let dir_response = ui.add(dir_box).on_hover_text("Flip direction of oscilloscope");

                                // Reset our line on change
                                if sync_response.clicked()
                                || dir_response.clicked()
                                || alt_sync_response.clicked()
                                || timing_response.changed()
                                {
                                    // Keep same direction when syncing (Issue #12)
                                    if sync_response.clicked() {
                                        // If flip selected already, it should be deselected on this click
                                        if dir_var.load(Ordering::SeqCst) {
                                            dir_var.store(false, Ordering::SeqCst);
                                        }
                                        // If flip not selected, it should now be selected
                                        else {
                                            dir_var.store(true, Ordering::SeqCst);
                                        }
                                    }
                                    sum_line = Line::new(PlotPoints::default());
                                    aux_line = Line::new(PlotPoints::default());
                                    aux_line_2 = Line::new(PlotPoints::default());
                                    aux_line_3 = Line::new(PlotPoints::default());
                                    aux_line_4 = Line::new(PlotPoints::default());
                                    aux_line_5 = Line::new(PlotPoints::default());
                                    scrolling_beat_line = Line::new(PlotPoints::default());
                                    line = Line::new(PlotPoints::default());
                                    samples.clear();
                                    aux_samples_1.clear();
                                    aux_samples_2.clear();
                                    aux_samples_3.clear();
                                    aux_samples_4.clear();
                                    aux_samples_5.clear();
                                    scrolling_beat_lines.clear();
                                }
                            }

                            if swap_response.clicked() {
                                let num = ontop.load(Ordering::SeqCst);
                                // This skips possible "OFF" lines when toggling
                                match num {
                                    0 => {
                                        if en_aux1.load(Ordering::SeqCst) {
                                            ontop.store(1, Ordering::SeqCst);
                                        } else if en_aux2.load(Ordering::SeqCst) {
                                            ontop.store(2, Ordering::SeqCst);
                                        } else if en_aux3.load(Ordering::SeqCst) {
                                            ontop.store(3, Ordering::SeqCst);
                                        } else if en_aux4.load(Ordering::SeqCst) {
                                            ontop.store(4, Ordering::SeqCst);
                                        } else if en_aux5.load(Ordering::SeqCst) {
                                            ontop.store(5, Ordering::SeqCst);
                                        }
                                    }
                                    1 => {
                                        if en_aux2.load(Ordering::SeqCst) {
                                            ontop.store(2, Ordering::SeqCst);
                                        } else if en_aux3.load(Ordering::SeqCst) {
                                            ontop.store(3, Ordering::SeqCst);
                                        } else if en_aux4.load(Ordering::SeqCst) {
                                            ontop.store(4, Ordering::SeqCst);
                                        } else if en_aux5.load(Ordering::SeqCst) {
                                            ontop.store(5, Ordering::SeqCst);
                                        } else if en_main.load(Ordering::SeqCst) {
                                            ontop.store(0, Ordering::SeqCst);
                                        }
                                    }
                                    2 => {
                                        if en_aux3.load(Ordering::SeqCst) {
                                            ontop.store(3, Ordering::SeqCst);
                                        } else if en_aux4.load(Ordering::SeqCst) {
                                            ontop.store(4, Ordering::SeqCst);
                                        } else if en_aux5.load(Ordering::SeqCst) {
                                            ontop.store(5, Ordering::SeqCst);
                                        } else if en_main.load(Ordering::SeqCst) {
                                            ontop.store(0, Ordering::SeqCst);
                                        } else if en_aux1.load(Ordering::SeqCst) {
                                            ontop.store(1, Ordering::SeqCst);
                                        }
                                    }
                                    3 => {
                                        if en_aux4.load(Ordering::SeqCst) {
                                            ontop.store(4, Ordering::SeqCst);
                                        } else if en_aux5.load(Ordering::SeqCst) {
                                            ontop.store(5, Ordering::SeqCst);
                                        } else if en_main.load(Ordering::SeqCst) {
                                            ontop.store(0, Ordering::SeqCst);
                                        } else if en_aux1.load(Ordering::SeqCst) {
                                            ontop.store(1, Ordering::SeqCst);
                                        } else if en_aux2.load(Ordering::SeqCst) {
                                            ontop.store(2, Ordering::SeqCst);
                                        }
                                    }
                                    4 => {
                                        if en_aux5.load(Ordering::SeqCst) {
                                            ontop.store(5, Ordering::SeqCst);
                                        } else if en_main.load(Ordering::SeqCst) {
                                            ontop.store(0, Ordering::SeqCst);
                                        } else if en_aux1.load(Ordering::SeqCst) {
                                            ontop.store(1, Ordering::SeqCst);
                                        } else if en_aux2.load(Ordering::SeqCst) {
                                            ontop.store(2, Ordering::SeqCst);
                                        } else if en_aux3.load(Ordering::SeqCst) {
                                            ontop.store(3, Ordering::SeqCst);
                                        }
                                    }
                                    5 => {
                                        if en_main.load(Ordering::SeqCst) {
                                            ontop.store(0, Ordering::SeqCst);
                                        } else if en_aux1.load(Ordering::SeqCst) {
                                            ontop.store(1, Ordering::SeqCst);
                                        } else if en_aux2.load(Ordering::SeqCst) {
                                            ontop.store(2, Ordering::SeqCst);
                                        } else if en_aux3.load(Ordering::SeqCst) {
                                            ontop.store(3, Ordering::SeqCst);
                                        } else if en_aux4.load(Ordering::SeqCst) {
                                            ontop.store(4, Ordering::SeqCst);
                                        }
                                    }
                                    _ => {
                                        // Not reachable
                                    }
                                }
                            }

                            ui.add(slim_checkbox::AtomicSlimCheckbox::new(
                                &en_main,
                                "In",
                            ));
                            ui.add(slim_checkbox::AtomicSlimCheckbox::new(
                                &en_aux1,
                                "2",
                            ));
                            ui.add(slim_checkbox::AtomicSlimCheckbox::new(
                                &en_aux2,
                                "3",
                            ));
                            ui.add(slim_checkbox::AtomicSlimCheckbox::new(
                                &en_aux3,
                                "4",
                            ));
                            ui.add(slim_checkbox::AtomicSlimCheckbox::new(
                                &en_aux4,
                                "5",
                            ));
                            ui.add(slim_checkbox::AtomicSlimCheckbox::new(
                                &en_aux5,
                                "6",
                            ));
                            if !show_analyzer.load(Ordering::SeqCst) {
                                ui.add(slim_checkbox::AtomicSlimCheckbox::new(
                                    &en_sum,
                                    "Sum",
                                ));
                            }
                            let analyzer_toggle = ui.add(slim_checkbox::AtomicSlimCheckbox::new(
                                &show_analyzer,
                                "Analyze",
                            ));
                            if show_analyzer.load(Ordering::SeqCst) {
                                ui.add(slim_checkbox::AtomicSlimCheckbox::new(
                                    &en_guidelines,
                                    "Guidelines",
                                ));
                                ui.add(slim_checkbox::AtomicSlimCheckbox::new(
                                    &en_bar_mode,
                                    "Bar Mode",
                                ));
                            }
                            if analyzer_toggle.clicked() {
                                // This is a ! because we'll always be behind the param toggle in time
                                if !show_analyzer.load(Ordering::SeqCst) {
                                    setter.set_parameter(&params.h_scale, prev_skip.load(Ordering::Relaxed));
                                } else {
                                    prev_skip.store(params.h_scale.value(), Ordering::Relaxed);
                                    if params.h_scale.value() > 1 {
                                        setter.set_parameter(&params.h_scale, 1)
                                    }
                                }
                            }         
                        });
                    });

                    // Reverse our order for drawing if desired (I know this is "slow")
                    if dir_var.load(Ordering::SeqCst) {
                        samples.make_contiguous().reverse();
                        aux_samples_1.make_contiguous().reverse();
                        aux_samples_2.make_contiguous().reverse();
                        aux_samples_3.make_contiguous().reverse();
                        aux_samples_4.make_contiguous().reverse();
                        aux_samples_5.make_contiguous().reverse();
                        scrolling_beat_lines.make_contiguous().reverse();
                    }

                    let mut final_primary_color: Color32 = Default::default();
                    let mut final_aux_line_color: Color32 = Default::default();
                    let mut final_aux_line_color_2: Color32 = Default::default();
                    let mut final_aux_line_color_3: Color32 = Default::default();
                    let mut final_aux_line_color_4: Color32 = Default::default();
                    let mut final_aux_line_color_5: Color32 = Default::default();

                    ui.allocate_ui(egui::Vec2::new(900.0, 380.0), |ui| {
                        // Fix our colors to focus on our line
                        let lmult: f32 = 0.25;
                        match ontop.load(Ordering::SeqCst) {
                            0 => {
                                // Main unaffected
                                final_primary_color = primary_line_color;
                                final_aux_line_color = user_aux_1.linear_multiply(lmult);
                                final_aux_line_color_2 = user_aux_2.linear_multiply(lmult);
                                final_aux_line_color_3 = user_aux_3.linear_multiply(lmult);
                                final_aux_line_color_4 = user_aux_4.linear_multiply(lmult);
                                final_aux_line_color_5 = user_aux_5.linear_multiply(lmult);
                            }
                            1 => {
                                // Aux unaffected
                                final_primary_color = primary_line_color.linear_multiply(lmult);
                                final_aux_line_color = user_aux_1;
                                final_aux_line_color_2 = user_aux_2.linear_multiply(lmult);
                                final_aux_line_color_3 = user_aux_3.linear_multiply(lmult);
                                final_aux_line_color_4 = user_aux_4.linear_multiply(lmult);
                                final_aux_line_color_5 = user_aux_5.linear_multiply(lmult);
                            }
                            2 => {
                                // Aux 2 unaffected
                                final_primary_color = primary_line_color.linear_multiply(lmult);
                                final_aux_line_color = user_aux_1.linear_multiply(lmult);
                                final_aux_line_color_2 = user_aux_2;
                                final_aux_line_color_3 = user_aux_3.linear_multiply(lmult);
                                final_aux_line_color_4 = user_aux_4.linear_multiply(lmult);
                                final_aux_line_color_5 = user_aux_5.linear_multiply(lmult);
                            }
                            3 => {
                                // Aux 3 unaffected
                                final_primary_color = primary_line_color.linear_multiply(lmult);
                                final_aux_line_color = user_aux_1.linear_multiply(lmult);
                                final_aux_line_color_2 = user_aux_2.linear_multiply(lmult);
                                final_aux_line_color_3 = user_aux_3;
                                final_aux_line_color_4 = user_aux_4.linear_multiply(lmult);
                                final_aux_line_color_5 = user_aux_5.linear_multiply(lmult);
                            }
                            4 => {
                                // Aux 4 unaffected
                                final_primary_color = primary_line_color.linear_multiply(lmult);
                                final_aux_line_color = user_aux_1.linear_multiply(lmult);
                                final_aux_line_color_2 = user_aux_2.linear_multiply(lmult);
                                final_aux_line_color_3 = user_aux_3.linear_multiply(lmult);
                                final_aux_line_color_4 = user_aux_4;
                                final_aux_line_color_5 = user_aux_5.linear_multiply(lmult);
                            }
                            5 => {
                                // Aux 5 unaffected
                                final_primary_color = primary_line_color.linear_multiply(lmult);
                                final_aux_line_color = user_aux_1.linear_multiply(lmult);
                                final_aux_line_color_2 = user_aux_2.linear_multiply(lmult);
                                final_aux_line_color_3 = user_aux_3.linear_multiply(lmult);
                                final_aux_line_color_4 = user_aux_4.linear_multiply(lmult);
                                final_aux_line_color_5 = user_aux_5;
                            }
                            _ => {
                                // We shouldn't be here
                            }
                        }

                        // Show the frequency analyzer
                        if show_analyzer.load(Ordering::SeqCst) {
                            let mut shapes = vec![];

                            // Compute our fast fourier transforms
                            let mut buffer: Vec<Complex<f32>> = samples.iter().map(|&x| Complex::new(x, 0.0)).collect();
                            let buffer_len: usize = buffer.len();
                            let fft_plan = fft.lock().unwrap().plan_fft(buffer_len, FftDirection::Forward);
                            fft_plan.process(&mut buffer);

                            let mut ax1: Vec<Complex<f32>> = aux_samples_1.iter().map(|&x| Complex::new(x, 0.0)).collect();
                            let ax1_len: usize = ax1.len();
                            let fft_plan = fft.lock().unwrap().plan_fft(ax1_len, FftDirection::Forward);
                            fft_plan.process(&mut ax1);

                            let mut ax2: Vec<Complex<f32>> = aux_samples_2.iter().map(|&x| Complex::new(x, 0.0)).collect();
                            let ax2_len: usize = ax2.len();
                            let fft_plan = fft.lock().unwrap().plan_fft(ax2_len, FftDirection::Forward);
                            fft_plan.process(&mut ax2);

                            let mut ax3: Vec<Complex<f32>> = aux_samples_3.iter().map(|&x| Complex::new(x, 0.0)).collect();
                            let ax3_len: usize = ax3.len();
                            let fft_plan = fft.lock().unwrap().plan_fft(ax3_len, FftDirection::Forward);
                            fft_plan.process(&mut ax3);

                            let mut ax4: Vec<Complex<f32>> = aux_samples_4.iter().map(|&x| Complex::new(x, 0.0)).collect();
                            let ax4_len: usize = ax4.len();
                            let fft_plan = fft.lock().unwrap().plan_fft(ax4_len, FftDirection::Forward);
                            fft_plan.process(&mut ax4);

                            let mut ax5: Vec<Complex<f32>> = aux_samples_5.iter().map(|&x| Complex::new(x, 0.0)).collect();
                            let ax5_len: usize = ax5.len();
                            let fft_plan = fft.lock().unwrap().plan_fft(ax5_len, FftDirection::Forward);
                            fft_plan.process(&mut ax5);

                            // Compute
                            let magnitudes: Vec<f32> = buffer.iter().map(|c| c.norm() as f32).collect();
                            let frequencies: Vec<f32> = (0..buffer_len / 2)
                                .map(|i| i as f32 * sr.load(Ordering::Relaxed) / buffer_len as f32)
                                .collect();
                            let magnitudes_ax1: Vec<f32> = ax1.iter().map(|c| c.norm() as f32).collect();
                            let frequencies_ax1: Vec<f32> = (0..ax1_len / 2)
                                .map(|i| i as f32 * sr.load(Ordering::Relaxed) / ax1_len as f32)
                                .collect();
                            let magnitudes_ax2: Vec<f32> = ax2.iter().map(|c| c.norm() as f32).collect();
                            let frequencies_ax2: Vec<f32> = (0..ax2_len / 2)
                                .map(|i| i as f32 * sr.load(Ordering::Relaxed) / ax2_len as f32)
                                .collect();
                            let magnitudes_ax3: Vec<f32> = ax3.iter().map(|c| c.norm() as f32).collect();
                            let frequencies_ax3: Vec<f32> = (0..ax3_len / 2)
                                .map(|i| i as f32 * sr.load(Ordering::Relaxed) / ax3_len as f32)
                                .collect();
                            let magnitudes_ax4: Vec<f32> = ax4.iter().map(|c| c.norm() as f32).collect();
                            let frequencies_ax4: Vec<f32> = (0..ax4_len / 2)
                                .map(|i| i as f32 * sr.load(Ordering::Relaxed) / ax4_len as f32)
                                .collect();
                            let magnitudes_ax5: Vec<f32> = ax5.iter().map(|c| c.norm() as f32).collect();
                            let frequencies_ax5: Vec<f32> = (0..ax5_len / 2)
                                .map(|i| i as f32 * sr.load(Ordering::Relaxed) / ax5_len as f32)
                                .collect();
                            // Scale for visibility
                            let db_scaler = 2.75;
                            let freq_scaler = 285.0;
                            let x_shift = -220.0;
                            let y_shift = 220.0;
                            // 1Khz pivot and -4.5 slope is same as Fruity Parametric EQ2
                            // For some reason 12 lines up the same here...
                            let pivot = 1000.0;
                            let slope = 12.0;

                            if en_bar_mode.load(Ordering::SeqCst) {
                                let length = frequencies.len();
                                //let bar_scaler = 300.0;
                                let bar_scaler = 1.6;
                                let bars: f32 = 64.0;
                                let chunk_size = length as f32 / bars;
                                let mut chunked_f: Vec<f32> = Vec::with_capacity(bars as usize);
                                let mut chunked_m: Vec<f32> = Vec::with_capacity(bars as usize);
                                let mut chunked_f_ax1: Vec<f32> = Vec::with_capacity(bars as usize);
                                let mut chunked_m_ax1: Vec<f32> = Vec::with_capacity(bars as usize);
                                let mut chunked_f_ax2: Vec<f32> = Vec::with_capacity(bars as usize);
                                let mut chunked_m_ax2: Vec<f32> = Vec::with_capacity(bars as usize);
                                let mut chunked_f_ax3: Vec<f32> = Vec::with_capacity(bars as usize);
                                let mut chunked_m_ax3: Vec<f32> = Vec::with_capacity(bars as usize);
                                let mut chunked_f_ax4: Vec<f32> = Vec::with_capacity(bars as usize);
                                let mut chunked_m_ax4: Vec<f32> = Vec::with_capacity(bars as usize);
                                let mut chunked_f_ax5: Vec<f32> = Vec::with_capacity(bars as usize);
                                let mut chunked_m_ax5: Vec<f32> = Vec::with_capacity(bars as usize);
                                for i in 0..bars as i32 {
                                    let start = (i as f32 * chunk_size) as usize;
                                    let end = if i == bars as i32 - 1 {
                                        length
                                    } else {
                                        ((i + 1) as f32 * chunk_size) as usize
                                    };

                                    let sum_f: f32 = frequencies[start..end].iter().sum();
                                    let average_f = sum_f / ((end - start) as f32);
                                    let sum_m: f32 = magnitudes[start..end].iter().sum();
                                    let average_m = sum_m / ((end - start) as f32);
                                    chunked_f.push(average_f);
                                    chunked_m.push(average_m);

                                    let sum_f_ax1: f32 = frequencies_ax1[start..end].iter().sum();
                                    let average_f_ax1 = sum_f_ax1 / ((end - start) as f32);
                                    let sum_m_ax1: f32 = magnitudes_ax1[start..end].iter().sum();
                                    let average_m_ax1 = sum_m_ax1 / ((end - start) as f32);
                                    chunked_f_ax1.push(average_f_ax1);
                                    chunked_m_ax1.push(average_m_ax1);

                                    let sum_f_ax2: f32 = frequencies_ax2[start..end].iter().sum();
                                    let average_f_ax2 = sum_f_ax2 / ((end - start) as f32);
                                    let sum_m_ax2: f32 = magnitudes_ax2[start..end].iter().sum();
                                    let average_m_ax2 = sum_m_ax2 / ((end - start) as f32);
                                    chunked_f_ax2.push(average_f_ax2);
                                    chunked_m_ax2.push(average_m_ax2);

                                    let sum_f_ax3: f32 = frequencies_ax3[start..end].iter().sum();
                                    let average_f_ax3 = sum_f_ax3 / ((end - start) as f32);
                                    let sum_m_ax3: f32 = magnitudes_ax3[start..end].iter().sum();
                                    let average_m_ax3 = sum_m_ax3 / ((end - start) as f32);
                                    chunked_f_ax3.push(average_f_ax3);
                                    chunked_m_ax3.push(average_m_ax3);

                                    let sum_f_ax4: f32 = frequencies_ax4[start..end].iter().sum();
                                    let average_f_ax4 = sum_f_ax4 / ((end - start) as f32);
                                    let sum_m_ax4: f32 = magnitudes_ax4[start..end].iter().sum();
                                    let average_m_ax4 = sum_m_ax4 / ((end - start) as f32);
                                    chunked_f_ax4.push(average_f_ax4);
                                    chunked_m_ax4.push(average_m_ax4);

                                    let sum_f_ax5: f32 = frequencies_ax5[start..end].iter().sum();
                                    let average_f_ax5 = sum_f_ax5 / ((end - start) as f32);
                                    let sum_m_ax5: f32 = magnitudes_ax5[start..end].iter().sum();
                                    let average_m_ax5 = sum_m_ax5 / ((end - start) as f32);
                                    chunked_f_ax5.push(average_f_ax5);
                                    chunked_m_ax5.push(average_m_ax5);
                                }

                                // Primary Input
                                let data: Vec<Pos2> = chunked_f
                                    .iter()
                                    .enumerate()
                                    .zip(chunked_m.iter())
                                    .map(|((i, freq), magnitude)| {
                                        let y = pivot_frequency_slope(*freq, *magnitude, pivot, slope);
                                        pos2(
                                            //freq.log10() * bar_scaler + x_shift,
                                            i as f32 * 10.0 * bar_scaler + 230.0,
                                            (util::gain_to_db(y) * -1.0) * db_scaler + y_shift
                                        )
                                    })
                                    .collect();

                                // Aux inputs
                                let data_ax1: Vec<Pos2> = chunked_f_ax1
                                    .iter()
                                    .enumerate()
                                    .zip(chunked_m_ax1.iter())
                                    .map(|((i, freq), magnitude)| {
                                        let y = pivot_frequency_slope(*freq, *magnitude, pivot, slope);
                                        pos2(
                                            //freq.log10() * bar_scaler + x_shift,
                                            i as f32 * 10.0 * bar_scaler + 230.0,
                                            (util::gain_to_db(y) * -1.0) * db_scaler + y_shift
                                        )
                                    })
                                    .collect();

                                let data_ax2: Vec<Pos2> = chunked_f_ax2
                                    .iter()
                                    .enumerate()
                                    .zip(chunked_m_ax2.iter())
                                    .map(|((i, freq), magnitude)| {
                                        let y = pivot_frequency_slope(*freq, *magnitude, pivot, slope);
                                        pos2(
                                            //freq.log10() * bar_scaler + x_shift,
                                            i as f32 * 10.0 * bar_scaler + 230.0,
                                            (util::gain_to_db(y) * -1.0) * db_scaler + y_shift
                                        )
                                    })
                                    .collect();

                                let data_ax3: Vec<Pos2> = chunked_f_ax3
                                    .iter()
                                    .enumerate()
                                    .zip(chunked_m_ax3.iter())
                                    .map(|((i, freq), magnitude)| {
                                        let y = pivot_frequency_slope(*freq, *magnitude, pivot, slope);
                                        pos2(
                                            //freq.log10() * bar_scaler + x_shift,
                                            i as f32 * 10.0 * bar_scaler + 230.0,
                                            (util::gain_to_db(y) * -1.0) * db_scaler + y_shift
                                        )
                                    })
                                    .collect();

                                let data_ax4: Vec<Pos2> = chunked_f_ax4
                                    .iter()
                                    .enumerate()
                                    .zip(chunked_m_ax4.iter())
                                    .map(|((i, freq), magnitude)| {
                                        let y = pivot_frequency_slope(*freq, *magnitude, pivot, slope);
                                        pos2(
                                            //freq.log10() * bar_scaler + x_shift,
                                            i as f32 * 10.0 * bar_scaler + 230.0,
                                            (util::gain_to_db(y) * -1.0) * db_scaler + y_shift
                                        )
                                    })
                                    .collect();

                                let data_ax5: Vec<Pos2> = chunked_f_ax5
                                    .iter()
                                    .enumerate()
                                    .zip(chunked_m_ax5.iter())
                                    .map(|((i, freq), magnitude)| {
                                        let y = pivot_frequency_slope(*freq, *magnitude, pivot, slope);
                                        pos2(
                                            //freq.log10() * bar_scaler + x_shift,
                                            i as f32 * 10.0 * bar_scaler + 230.0,
                                            (util::gain_to_db(y) * -1.0) * db_scaler + y_shift
                                        )
                                    })
                                    .collect();

                                    // Draw whichever order next
                                    match ontop.load(Ordering::SeqCst) {
                                        0 => {
                                            if en_aux5.load(Ordering::SeqCst) { 
                                                for elem in data_ax5.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_5
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux4.load(Ordering::SeqCst) { 
                                                for elem in data_ax4.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_4
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux3.load(Ordering::SeqCst) { 
                                                for elem in data_ax3.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_3
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux2.load(Ordering::SeqCst) { 
                                                for elem in data_ax2.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_2
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux1.load(Ordering::SeqCst) { 
                                                for elem in data_ax1.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_1
                                                        )
                                                    );
                                                }
                                            }
                                            if en_main.load(Ordering::SeqCst) { 
                                                for elem in data.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            final_primary_color
                                                        )
                                                    );
                                                }
                                            }
                                        }
                                        1 => {
                                            if en_main.load(Ordering::SeqCst) { 
                                                for elem in data.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            final_primary_color
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux5.load(Ordering::SeqCst) { 
                                                for elem in data_ax5.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_5
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux4.load(Ordering::SeqCst) { 
                                                for elem in data_ax4.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_4
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux3.load(Ordering::SeqCst) { 
                                                for elem in data_ax3.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_3
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux2.load(Ordering::SeqCst) { 
                                                for elem in data_ax2.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_2
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux1.load(Ordering::SeqCst) { 
                                                for elem in data_ax1.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_1
                                                        )
                                                    );
                                                }
                                            }
                                        }
                                        2 => {
                                            if en_aux1.load(Ordering::SeqCst) { 
                                                for elem in data_ax1.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_1
                                                        )
                                                    );
                                                }
                                            }
                                            if en_main.load(Ordering::SeqCst) { 
                                                for elem in data.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            final_primary_color
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux5.load(Ordering::SeqCst) { 
                                                for elem in data_ax5.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_5
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux4.load(Ordering::SeqCst) { 
                                                for elem in data_ax4.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_4
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux3.load(Ordering::SeqCst) { 
                                                for elem in data_ax3.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_3
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux2.load(Ordering::SeqCst) { 
                                                for elem in data_ax2.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_2
                                                        )
                                                    );
                                                }
                                            }
                                        }
                                        3 => {
                                            if en_aux2.load(Ordering::SeqCst) { 
                                                for elem in data_ax2.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_2
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux1.load(Ordering::SeqCst) { 
                                                for elem in data_ax1.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_1
                                                        )
                                                    );
                                                }
                                            }
                                            if en_main.load(Ordering::SeqCst) { 
                                                for elem in data.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            final_primary_color
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux5.load(Ordering::SeqCst) { 
                                                for elem in data_ax5.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_5
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux4.load(Ordering::SeqCst) { 
                                                for elem in data_ax4.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_4
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux3.load(Ordering::SeqCst) { 
                                                for elem in data_ax3.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_3
                                                        )
                                                    );
                                                }
                                            }
                                        }
                                        4 => {
                                            if en_aux3.load(Ordering::SeqCst) { 
                                                for elem in data_ax3.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_3
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux2.load(Ordering::SeqCst) { 
                                                for elem in data_ax2.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_2
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux1.load(Ordering::SeqCst) { 
                                                for elem in data_ax1.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_1
                                                        )
                                                    );
                                                }
                                            }
                                            if en_main.load(Ordering::SeqCst) { 
                                                for elem in data.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            final_primary_color
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux5.load(Ordering::SeqCst) { 
                                                for elem in data_ax5.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_5
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux4.load(Ordering::SeqCst) { 
                                                for elem in data_ax4.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_4
                                                        )
                                                    );
                                                }
                                            }
                                        }
                                        5 => {
                                            if en_aux4.load(Ordering::SeqCst) { 
                                                for elem in data_ax4.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_4
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux3.load(Ordering::SeqCst) { 
                                                for elem in data_ax3.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_3
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux2.load(Ordering::SeqCst) { 
                                                for elem in data_ax2.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_2
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux1.load(Ordering::SeqCst) { 
                                                for elem in data_ax1.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_1
                                                        )
                                                    );
                                                }
                                            }
                                            if en_main.load(Ordering::SeqCst) { 
                                                for elem in data.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            final_primary_color
                                                        )
                                                    );
                                                }
                                            }
                                            if en_aux5.load(Ordering::SeqCst) { 
                                                for elem in data_ax5.iter() {
                                                    shapes.push(
                                                        epaint::Shape::rect_filled(
                                                            Rect { 
                                                                min: Pos2::new(elem.x + x_shift, elem.y), 
                                                                max: Pos2::new(elem.x + 10.0 + x_shift, 515.0)
                                                            },
                                                            Rounding::none(),
                                                            user_aux_5
                                                        )
                                                    );
                                                }
                                            }
                                        }
                                        _ => {
                                            // We shouldn't be here
                                        }
                                    }

                                ui.painter().extend(shapes);
                            } else {
                                // Primary Input
                                let data: Vec<Pos2> = frequencies
                                    .iter()
                                    .zip(magnitudes.iter())
                                    .map(|(freq, magnitude)| {
                                        let y = pivot_frequency_slope(*freq, *magnitude, pivot, slope);
                                        pos2(
                                            freq.log10() * freq_scaler + x_shift,
                                            (util::gain_to_db(y) * -1.0) * db_scaler + y_shift
                                        )
                                    })
                                    .collect();

                                // Aux
                                let ax1_data: Vec<Pos2> = frequencies_ax1
                                    .iter()
                                    .zip(magnitudes_ax1.iter())
                                    .map(|(freq, magnitude)| {
                                        let y = pivot_frequency_slope(*freq, *magnitude, pivot, slope);
                                        pos2(
                                            freq.log10() * freq_scaler + x_shift,
                                            (util::gain_to_db(y) * -1.0) * db_scaler + y_shift
                                        )
                                    })
                                    .collect();

                                let ax2_data: Vec<Pos2> = frequencies_ax2
                                    .iter()
                                    .zip(magnitudes_ax2.iter())
                                    .map(|(freq, magnitude)| {
                                        let y = pivot_frequency_slope(*freq, *magnitude, pivot, slope);
                                        pos2(
                                            freq.log10() * freq_scaler + x_shift,
                                            (util::gain_to_db(y) * -1.0) * db_scaler + y_shift
                                        )
                                    })
                                    .collect();

                                let ax3_data: Vec<Pos2> = frequencies_ax3
                                    .iter()
                                    .zip(magnitudes_ax3.iter())
                                    .map(|(freq, magnitude)| {
                                        let y = pivot_frequency_slope(*freq, *magnitude, pivot, slope);
                                        pos2(
                                            freq.log10() * freq_scaler + x_shift,
                                            (util::gain_to_db(y) * -1.0) * db_scaler + y_shift
                                        )
                                    })
                                    .collect();

                                let ax4_data: Vec<Pos2> = frequencies_ax4
                                    .iter()
                                    .zip(magnitudes_ax4.iter())
                                    .map(|(freq, magnitude)| {
                                        let y = pivot_frequency_slope(*freq, *magnitude, pivot, slope);
                                        pos2(
                                            freq.log10() * freq_scaler + x_shift,
                                            (util::gain_to_db(y) * -1.0) * db_scaler + y_shift
                                        )
                                    })
                                    .collect();

                                let ax5_data: Vec<Pos2> = frequencies_ax5
                                    .iter()
                                    .zip(magnitudes_ax5.iter())
                                    .map(|(freq, magnitude)| {
                                        let y = pivot_frequency_slope(*freq, *magnitude, pivot, slope);
                                        pos2(
                                            freq.log10() * freq_scaler + x_shift,
                                            (util::gain_to_db(y) * -1.0) * db_scaler + y_shift
                                        )
                                    })
                                    .collect();

                                if en_guidelines.load(Ordering::SeqCst) {
                                    let freqs: [f32; 12] = [
                                        0.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 18000.0];
                                    let scaled_ref_freqs: Vec<f32> = freqs.iter().map(|num|{num.log10() * freq_scaler}).collect();
                                    for (num,scaled_num) in freqs.iter().zip(scaled_ref_freqs.iter()) {
                                        shapes.push(
                                            epaint::Shape::line_segment(
                                                [
                                                    Pos2::new(*scaled_num + x_shift, 515.0),
                                                    Pos2::new(*scaled_num + x_shift, 30.0)
                                                ],
                                                Stroke::new(0.5, Color32::GRAY)
                                            )
                                        );
                                        ui.painter().text(
                                            Pos2::new(scaled_num + 2.0 + x_shift, 510.0), 
                                            Align2::LEFT_CENTER, 
                                            *num, 
                                            FontId::monospace(12.0), 
                                            Color32::GRAY
                                        );
                                    }
                                    let sub_freqs: [f32; 18] = [
                                        30.0,40.0,60.0,70.0,80.0,90.0,300.0,400.0,600.0,700.0,800.0,900.0,3000.0,4000.0,6000.0,7000.0,8000.0,9000.0];
                                    let scaled_sub_freqs: Vec<f32> = sub_freqs.iter().map(|num|{num.log10() * freq_scaler}).collect();
                                    for scaled_num in scaled_sub_freqs.iter() {
                                        shapes.push(
                                            epaint::Shape::line_segment(
                                                [
                                                    Pos2::new(*scaled_num + x_shift, 515.0),
                                                    Pos2::new(*scaled_num + x_shift, 30.0)
                                                ],
                                                Stroke::new(0.5, Color32::DARK_GRAY)
                                            )
                                        );
                                    }
                                }

                                // Draw whichever order next
                                match ontop.load(Ordering::SeqCst) {
                                    0 => {
                                        if en_aux5.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax5_data, Stroke::new(1.0, final_aux_line_color_5))); }
                                        if en_aux4.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax4_data, Stroke::new(1.0, final_aux_line_color_4))); }
                                        if en_aux3.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax3_data, Stroke::new(1.0, final_aux_line_color_3))); }
                                        if en_aux2.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax2_data, Stroke::new(1.0, final_aux_line_color_2))); }
                                        if en_aux1.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax1_data, Stroke::new(1.0, final_aux_line_color))); }
                                        if en_main.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(data, Stroke::new(1.0, final_primary_color))); }
                                    }
                                    1 => {
                                        if en_main.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(data, Stroke::new(1.0, final_primary_color))); }
                                        if en_aux5.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax5_data, Stroke::new(1.0, final_aux_line_color_5))); }
                                        if en_aux4.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax4_data, Stroke::new(1.0, final_aux_line_color_4))); }
                                        if en_aux3.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax3_data, Stroke::new(1.0, final_aux_line_color_3))); }
                                        if en_aux2.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax2_data, Stroke::new(1.0, final_aux_line_color_2))); }
                                        if en_aux1.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax1_data, Stroke::new(1.0, final_aux_line_color))); }
                                    }
                                    2 => {
                                        if en_aux1.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax1_data, Stroke::new(1.0, final_aux_line_color))); }
                                        if en_main.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(data, Stroke::new(1.0, final_primary_color))); }
                                        if en_aux5.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax5_data, Stroke::new(1.0, final_aux_line_color_5))); }
                                        if en_aux4.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax4_data, Stroke::new(1.0, final_aux_line_color_4))); }
                                        if en_aux3.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax3_data, Stroke::new(1.0, final_aux_line_color_3))); }
                                        if en_aux2.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax2_data, Stroke::new(1.0, final_aux_line_color_2))); }
                                    }
                                    3 => {
                                        if en_aux2.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax2_data, Stroke::new(1.0, final_aux_line_color_2))); }
                                        if en_aux1.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax1_data, Stroke::new(1.0, final_aux_line_color))); }
                                        if en_main.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(data, Stroke::new(1.0, final_primary_color))); }
                                        if en_aux5.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax5_data, Stroke::new(1.0, final_aux_line_color_5))); }
                                        if en_aux4.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax4_data, Stroke::new(1.0, final_aux_line_color_4))); }
                                        if en_aux3.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax3_data, Stroke::new(1.0, final_aux_line_color_3))); }
                                    }
                                    4 => {
                                        if en_aux3.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax3_data, Stroke::new(1.0, final_aux_line_color_3))); }
                                        if en_aux2.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax2_data, Stroke::new(1.0, final_aux_line_color_2))); }
                                        if en_aux1.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax1_data, Stroke::new(1.0, final_aux_line_color))); }
                                        if en_main.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(data, Stroke::new(1.0, final_primary_color))); }
                                        if en_aux5.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax5_data, Stroke::new(1.0, final_aux_line_color_5))); }
                                        if en_aux4.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax4_data, Stroke::new(1.0, final_aux_line_color_4))); }
                                    }
                                    5 => {
                                        if en_aux4.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax4_data, Stroke::new(1.0, final_aux_line_color_4))); }
                                        if en_aux3.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax3_data, Stroke::new(1.0, final_aux_line_color_3))); }
                                        if en_aux2.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax2_data, Stroke::new(1.0, final_aux_line_color_2))); }
                                        if en_aux1.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax1_data, Stroke::new(1.0, final_aux_line_color))); }
                                        if en_main.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(data, Stroke::new(1.0, final_primary_color))); }
                                        if en_aux5.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax5_data, Stroke::new(1.0, final_aux_line_color_5))); }
                                    }
                                    _ => {
                                        // We shouldn't be here
                                    }
                                }

                                ui.painter().extend(shapes);
                            }
                        } else {
                            /*
                            if alt_sync.load(Ordering::SeqCst) {
                                while samples.back().map_or(false, |&x| x == 0.0) {
                                    samples.pop_back();
                                }
                                while aux_samples_1.back().map_or(false, |&x| x == 0.0) {
                                    aux_samples_1.pop_back();
                                }
                                while aux_samples_2.back().map_or(false, |&x| x == 0.0) {
                                    aux_samples_2.pop_back();
                                }
                                while aux_samples_3.back().map_or(false, |&x| x == 0.0) {
                                    aux_samples_3.pop_back();
                                }
                                while aux_samples_4.back().map_or(false, |&x| x == 0.0) {
                                    aux_samples_4.pop_back();
                                }
                                while aux_samples_5.back().map_or(false, |&x| x == 0.0) {
                                    aux_samples_5.pop_back();
                                }
                            }
                            */
                            let mut sum_data = samples.clone();

                            let sbl: PlotPoints = scrolling_beat_lines
                                .iter()
                                .enumerate()
                                .map(|(i, sample)| {
                                    [i as f64, *sample as f64]
                                })
                                .collect();
                            let sbl_line = Line::new(sbl)
                                .color(guidelines)
                                .stroke(Stroke::new(0.25, guidelines.linear_multiply(0.5)));

                            // Primary Input
                            let data: PlotPoints = samples
                                .iter()
                                .enumerate()
                                .map(|(i, sample)| {
                                    let x: f64;
                                    let y: f64;
                                    if en_main.load(Ordering::SeqCst) {
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
                                    if en_aux1.load(Ordering::SeqCst) {
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
                                .color(user_aux_1)
                                .stroke(Stroke::new(1.0, user_aux_1));

                            let aux_data_2: PlotPoints = aux_samples_2
                                .iter()
                                .enumerate()
                                .map(|(i, sample)| {
                                    let x: f64;
                                    let y: f64;
                                    if en_aux2.load(Ordering::SeqCst) {
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
                                .color(user_aux_2)
                                .stroke(Stroke::new(1.0, user_aux_2));

                            let aux_data_3: PlotPoints = aux_samples_3
                                .iter()
                                .enumerate()
                                .map(|(i, sample)| {
                                    let x: f64;
                                    let y: f64;
                                    if en_aux3.load(Ordering::SeqCst) {
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
                                .color(user_aux_3)
                                .stroke(Stroke::new(1.0, user_aux_3));

                            let aux_data_4: PlotPoints = aux_samples_4
                                .iter()
                                .enumerate()
                                .map(|(i, sample)| {
                                    let x: f64;
                                    let y: f64;
                                    if en_aux4.load(Ordering::SeqCst) {
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
                                .color(user_aux_4)
                                .stroke(Stroke::new(1.0, user_aux_4));

                            let aux_data_5: PlotPoints = aux_samples_5
                                .iter()
                                .enumerate()
                                .map(|(i, sample)| {
                                    let x: f64;
                                    let y: f64;
                                    if en_aux5.load(Ordering::SeqCst) {
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
                                .color(user_aux_5)
                                .stroke(Stroke::new(1.0, user_aux_5));

                            if en_sum.load(Ordering::SeqCst) {
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
                                    .color(user_sum_line.linear_multiply(0.25))
                                    .stroke(Stroke::new(0.9, user_sum_line));
                            }

                            // Show the Oscilloscope
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
                                    plot_ui.line(sbl_line);

                                    if en_sum.load(Ordering::SeqCst) {
                                        // Draw the sum line first so it's furthest behind
                                        plot_ui.line(sum_line);
                                    }

                                    // Figure out the lines to draw

                                    // Draw whichever order next
                                    match ontop.load(Ordering::SeqCst) {
                                        0 => {
                                            if en_aux5.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_5);
                                            }
                                            if en_aux4.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_4);
                                            }
                                            if en_aux3.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_3);
                                            }
                                            if en_aux2.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_2);
                                            }
                                            if en_aux1.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line);
                                            }
                                            if en_main.load(Ordering::SeqCst) {
                                                plot_ui.line(line);
                                            }
                                        }
                                        1 => {
                                            if en_main.load(Ordering::SeqCst) {
                                                plot_ui.line(line);
                                            }
                                            if en_aux5.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_5);
                                            }
                                            if en_aux4.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_4);
                                            }
                                            if en_aux3.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_3);
                                            }
                                            if en_aux2.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_2);
                                            }
                                            if en_aux1.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line);
                                            }
                                        }
                                        2 => {
                                            if en_aux1.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line);
                                            }
                                            if en_main.load(Ordering::SeqCst) {
                                                plot_ui.line(line);
                                            }
                                            if en_aux5.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_5);
                                            }
                                            if en_aux4.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_4);
                                            }
                                            if en_aux3.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_3);
                                            }
                                            if en_aux2.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_2);
                                            }
                                        }
                                        3 => {
                                            if en_aux2.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_2);
                                            }
                                            if en_aux1.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line);
                                            }
                                            if en_main.load(Ordering::SeqCst) {
                                                plot_ui.line(line);
                                            }
                                            if en_aux5.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_5);
                                            }
                                            if en_aux4.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_4);
                                            }
                                            if en_aux3.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_3);
                                            }
                                        }
                                        4 => {
                                            if en_aux3.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_3);
                                            }
                                            if en_aux2.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_2);
                                            }
                                            if en_aux1.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line);
                                            }
                                            if en_main.load(Ordering::SeqCst) {
                                                plot_ui.line(line);
                                            }
                                            if en_aux5.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_5);
                                            }
                                            if en_aux4.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_4);
                                            }
                                        }
                                        5 => {
                                            if en_aux4.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_4);
                                            }
                                            if en_aux3.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_3);
                                            }
                                            if en_aux2.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line_2);
                                            }
                                            if en_aux1.load(Ordering::SeqCst) {
                                                plot_ui.line(aux_line);
                                            }
                                            if en_main.load(Ordering::SeqCst) {
                                                plot_ui.line(line);
                                            }
                                            if en_aux5.load(Ordering::SeqCst) {
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
                        }
                    });

                    // Put things back after drawing so process() isn't broken
                    if dir_var.load(Ordering::SeqCst) {
                        samples.make_contiguous().reverse();
                        aux_samples_1.make_contiguous().reverse();
                        aux_samples_2.make_contiguous().reverse();
                        aux_samples_3.make_contiguous().reverse();
                        aux_samples_4.make_contiguous().reverse();
                        aux_samples_5.make_contiguous().reverse();
                        scrolling_beat_lines.make_contiguous().reverse();
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
            if sample_rate != self.sample_rate.load(Ordering::Relaxed) {
                self.sample_rate.store(sample_rate, Ordering::Relaxed);
            }
            // Reset this every buffer process
            self.skip_counter = 0;

            // Get iterators outside the loop
            // These are immutable to not break borrows and the .to_iter() things that return borrows
            let raw_buffer = buffer.as_slice_immutable();
            let aux_0 = aux.inputs[0].as_slice_immutable();
            let aux_1 = aux.inputs[1].as_slice_immutable();
            let aux_2 = aux.inputs[2].as_slice_immutable();
            let aux_3 = aux.inputs[3].as_slice_immutable();
            let aux_4 = aux.inputs[4].as_slice_immutable();

            if !self.show_analyzer.load(Ordering::SeqCst) {
                for (b0, ax0, ax1, ax2, ax3, ax4) in
                    izip!(raw_buffer, aux_0, aux_1, aux_2, aux_3, aux_4)
                {
                    let current_beat: f64 = context.transport().pos_beats().unwrap();
                    let temp_current_beat: f64 = (current_beat * 10000.0 as f64).round() / 10000.0 as f64;
                    let offset: i64 = 1000;
                    let sample_pos: i64;
                    let sample_pos_round: f64;
                    let mut time_seconds: f64 = 0.0;
                    let beat_length_seconds: f64;
                    let mut expected_beat_times: Vec<f64> = Vec::new();
                    let tolerance: f64 = 0.021;
                    let mut is_on_beat: bool = false;
                    if self.alt_sync.load(Ordering::SeqCst) {
                        sample_pos = context.transport().pos_samples().unwrap() + offset;
                        sample_pos_round = (sample_pos as f64 * 1000.0 as f64).round() / 1000.0 as f64;
                        time_seconds = sample_pos_round as f64 / self.sample_rate.load(Ordering::SeqCst) as f64;
                        beat_length_seconds = 60.0 / context.transport().tempo.unwrap();
                        expected_beat_times = (0..).map(|i| i as f64 * beat_length_seconds).take_while(|&t| t < time_seconds).collect();
                        is_on_beat = expected_beat_times.iter().any(|&beat_time| (time_seconds - beat_time).abs() < tolerance);
                        if context.transport().playing && is_on_beat
                        {
                            self.add_beat_line.store(true, Ordering::SeqCst);
                        }
                    } else if temp_current_beat % 1.0 == 0.0 && context.transport().playing {
                        self.add_beat_line.store(true, Ordering::SeqCst);
                    }
                    // Beat syncing control
                    if self.sync_var.load(Ordering::SeqCst) {
                        if self.alt_sync.load(Ordering::SeqCst) {
                            if context.transport().playing {
                                match self.params.sync_timing.value() {
                                    BeatSync::Bar => {
                                        let is_on_bar = expected_beat_times.iter().any(|&beat_time| (time_seconds - (beat_time * 4.0)).abs() < tolerance);
                                        if is_on_bar {
                                            if self.beat_threshold.load(Ordering::SeqCst) == 0 {
                                                self.in_place_index.store(0, Ordering::SeqCst);
                                                self.beat_threshold.fetch_add(1, Ordering::SeqCst);
                                            }
                                        } else {
                                            if self.beat_threshold.load(Ordering::SeqCst) > 0 {
                                                self.beat_threshold.store(0, Ordering::SeqCst);
                                            }
                                        }
                                    },
                                    BeatSync::Beat => {
                                        if is_on_beat {
                                            if self.beat_threshold.load(Ordering::SeqCst) == 0 {
                                                self.in_place_index.store(0, Ordering::SeqCst);
                                                self.beat_threshold.fetch_add(1, Ordering::SeqCst);
                                            }
                                        } else {
                                            if self.beat_threshold.load(Ordering::SeqCst) > 0 {
                                                self.beat_threshold.store(0, Ordering::SeqCst);
                                            }
                                        }
                                    }
                                }
                            } else {
                                self.in_place_index.store(0, Ordering::SeqCst);
                            }
                        } else {
                            // Works in FL Studio but not other daws, hence the previous couple of lines
                            match self.params.sync_timing.value() {
                                BeatSync::Bar => {
                                    if temp_current_beat % 4.0 == 0.0 {
                                        // Reset our index to the sample vecdeques
                                        //self.in_place_index = Arc::new(Mutex::new(0));
                                        self.in_place_index.store(0, Ordering::SeqCst);
                                        self.skip_counter = 0;
                                    }
                                }
                                BeatSync::Beat => {
                                    if temp_current_beat % 1.0 == 0.0 {
                                        // Reset our index to the sample vecdeques
                                        //self.in_place_index = Arc::new(Mutex::new(0));
                                        self.in_place_index.store(0, Ordering::SeqCst);
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
                        b0.iter(),
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
                            let mut sbl_guard = self.scrolling_beat_lines.lock().unwrap();
                            // If beat sync is on, we need to process changes in place
                            if self.sync_var.load(Ordering::SeqCst) {
                                // Access the in place index
                                let ipi_index: usize = self.in_place_index.load(Ordering::SeqCst) as usize;
                                // If we add a beat line, also clean all VecDeques past this index to line them up
                                if self.add_beat_line.load(Ordering::SeqCst) {
                                    sbl_guard.push_front(1.0);
                                    sbl_guard.push_front(-1.0);
                                    self.add_beat_line.store(false, Ordering::SeqCst);
                                    if self.alt_sync.load(Ordering::SeqCst) && self.params.sync_timing.value() == BeatSync::Beat {
                                        // This removes extra stuff on the right (jitter)
                                        guard.drain(ipi_index..);
                                        aux_guard.drain(ipi_index..);
                                        aux_guard_2.drain(ipi_index..);
                                        aux_guard_3.drain(ipi_index..);
                                        aux_guard_4.drain(ipi_index..);
                                        aux_guard_5.drain(ipi_index..);
                                    }
                                } else {
                                    sbl_guard.push_front(0.0);
                                }
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
                                self.in_place_index.fetch_add(1, Ordering::SeqCst);
                            }
                            // Beat sync is off: allow "scroll"
                            else {
                                if self.add_beat_line.load(Ordering::SeqCst) {
                                    sbl_guard.push_front(1.0);
                                    sbl_guard.push_front(-1.0);
                                    self.add_beat_line.store(false, Ordering::SeqCst);
                                } else {
                                    sbl_guard.push_front(0.0);
                                }
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
                            if sbl_guard.len() != scroll {
                                sbl_guard.resize(scroll, 0.0);
                            }
                        }
                        self.skip_counter += 1;
                    }
                }
            } else {
                for (b0, ax0, ax1, ax2, ax3, ax4) in
                    izip!(raw_buffer, aux_0, aux_1, aux_2, aux_3, aux_4)
                {
                    if self.skip_counter % self.params.h_scale.value() == 0 {
                        for (
                            sample,
                            aux_sample_1,
                            aux_sample_2,
                            aux_sample_3,
                            aux_sample_4,
                            aux_sample_5,
                        ) in izip!(
                            b0.iter(),
                            ax0.iter(),
                            ax1.iter(),
                            ax2.iter(),
                            ax3.iter(),
                            ax4.iter()
                        ) {
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

                            // Update our main samples vector for oscilloscope drawing
                            let mut guard = self.samples.lock().unwrap();
                            // Update our sidechain samples vector for oscilloscope drawing
                            let mut aux_guard = self.aux_samples_1.lock().unwrap();
                            let mut aux_guard_2 = self.aux_samples_2.lock().unwrap();
                            let mut aux_guard_3 = self.aux_samples_3.lock().unwrap();
                            let mut aux_guard_4 = self.aux_samples_4.lock().unwrap();
                            let mut aux_guard_5 = self.aux_samples_5.lock().unwrap();

                            guard.push_front(visual_main_sample);
                            aux_guard.push_front(visual_aux_sample_1);
                            aux_guard_2.push_front(visual_aux_sample_2);
                            aux_guard_3.push_front(visual_aux_sample_3);
                            aux_guard_4.push_front(visual_aux_sample_4);
                            aux_guard_5.push_front(visual_aux_sample_5);

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


fn pivot_frequency_slope(freq: f32, magnitude: f32, f0: f32, slope: f32) -> f32{
    if freq < f0 {
        magnitude * (freq / f0).powf(slope / 20.0)
    } else {
        magnitude * (f0 / freq).powf(slope / 20.0)
    }
}
