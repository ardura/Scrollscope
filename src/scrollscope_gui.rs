use configparser::ini::Ini;
use egui_plot::{HLine, Line, Plot, PlotPoints};
use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{
        self, epaint::{self}, pos2, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, Stroke, UiBuilder
    },
    widgets,
};
use rustfft::{num_complex::Complex, Fft, FftDirection};
use std::{fs::File, io::Write, path::MAIN_SEPARATOR_STR, str::FromStr, sync::{atomic::Ordering, Arc}};
use std::ops::RangeInclusive;
use crate::{pivot_frequency_slope, slim_checkbox, Scrollscope};

pub(crate) fn make_gui(instance: &mut Scrollscope, _async_executor: AsyncExecutor<Scrollscope>) -> Option<Box<dyn Editor>> {
    let params = instance.params.clone();
    let samples = instance.sample_buffer.clone();
    let samples_2 = instance.sample_buffer_2.clone();
    let ontop = instance.focused_line_toggle.clone();
    let is_clipping = instance.is_clipping.clone();
    let sync_var = instance.sync_var.clone();
    let alt_sync = instance.alt_sync.clone();
    let dir_var = instance.direction.clone();
    let en_main = instance.channel_enabled[0].clone();
    let en_aux1 = instance.channel_enabled[1].clone();
    let en_aux2 = instance.channel_enabled[2].clone();
    let en_aux3 = instance.channel_enabled[3].clone();
    let en_aux4 = instance.channel_enabled[4].clone();
    let en_aux5 = instance.channel_enabled[5].clone();
    let en_sum = instance.enable_sum.clone();
    let en_guidelines = instance.enable_guidelines.clone();
    let en_bar_mode = instance.enable_bar_mode.clone();
    let fft = instance.fft.clone();
    let show_analyzer = instance.show_analyzer.clone();
    let en_filled_lines = instance.en_filled_lines.clone();
    let en_filled_osc = instance.en_filled_osc.clone();
    let stereo_view = instance.stereo_view.clone();
    let sample_rate = instance.sample_rate.clone();
    let prev_skip = instance.prev_skip.clone();
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

    // Setup highlights and other color variations ahead of time to save processing
    let soften = 0.25;
    let soft_primary = primary_line_color.linear_multiply(soften);
    let soft_aux_1 =  user_aux_1.linear_multiply(soften);
    let soft_aux_2 = user_aux_2.linear_multiply(soften);
    let soft_aux_3 = user_aux_3.linear_multiply(soften);
    let soft_aux_4 = user_aux_4.linear_multiply(soften);
    let soft_aux_5 = user_aux_5.linear_multiply(soften);
    
    create_egui_editor(
        instance.params.editor_state.clone(),
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
                    .rect_filled(Rect::EVERYTHING, CornerRadius::ZERO, background_color);
                //ui.set_style(style_var);
                // Reset these to be assigned/reassigned when params change
                let mut sum_line: Line = Line::new(PlotPoints::default());
                let mut aux_line: Line = Line::new(PlotPoints::default());
                let mut aux_line_2: Line = Line::new(PlotPoints::default());
                let mut aux_line_3: Line = Line::new(PlotPoints::default());
                let mut aux_line_4: Line = Line::new(PlotPoints::default());
                let mut aux_line_5: Line = Line::new(PlotPoints::default());
                let mut sum_line_2: Line = Line::new(PlotPoints::default());
                #[allow(non_snake_case)]
                let mut aux_line__2: Line = Line::new(PlotPoints::default());
                let mut aux_line_2_2: Line = Line::new(PlotPoints::default());
                let mut aux_line_3_2: Line = Line::new(PlotPoints::default());
                let mut aux_line_4_2: Line = Line::new(PlotPoints::default());
                let mut aux_line_5_2: Line = Line::new(PlotPoints::default());
                let mut scrolling_beat_line: Line = Line::new(PlotPoints::default());
                let mut line: Line = Line::new(PlotPoints::default());
                let mut line_2: Line = Line::new(PlotPoints::default());
                let sr = sample_rate.clone();
                // The entire "window" container
                ui.vertical(|ui| {
                    // This is the top bar
                    ui.horizontal(|ui| {
                        ui.label("Scrollscope")
                            .on_hover_text("by Ardura with nih-plug and egui
Version 1.4.1");
                        ui.add(
                            widgets::ParamSlider::for_param(&params.free_gain, setter)
                                .with_width(30.0),
                        ).on_hover_text("Visual gain adjustment (no output change)");
                        ui.add_space(4.0);
                        let swap_response: Response;
                        if show_analyzer.load(Ordering::SeqCst) {
                            let _scroll_handle = ui.add(
                                widgets::ParamSlider::for_param(&params.scrollspeed, setter)
                                    .with_width(120.0),
                            );
                            ui.add_space(4.0);
                            
                            swap_response = ui
                                .button("Toggle")
                                .on_hover_text("Change the drawing order of waveforms");
                        } else {
                            let _scroll_handle = ui.add(
                                widgets::ParamSlider::for_param(&params.scrollspeed, setter)
                                    .with_width(50.0),
                            ).on_hover_text("The amount of time the oscilloscope uses to capture data");

                            ui.add_space(4.0);

                            let _scale_handle = ui.add(
                                widgets::ParamSlider::for_param(&params.h_scale, setter)
                                    .with_width(30.0),
                            ).on_hover_text("How many samples are skipped before reading a value (use this to optimize)");
                            ui.add_space(4.0);
                            swap_response = ui
                                .button("Toggle")
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
                            let fill_osc = slim_checkbox::AtomicSlimCheckbox::new(&en_filled_osc, "Fill");
                            let _fill_response = ui.add(fill_osc).on_hover_text("Fill the oscilloscope drawing");
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
                                sum_line_2 = Line::new(PlotPoints::default());
                                aux_line__2 = Line::new(PlotPoints::default());
                                aux_line_2_2 = Line::new(PlotPoints::default());
                                aux_line_3_2 = Line::new(PlotPoints::default());
                                aux_line_4_2 = Line::new(PlotPoints::default());
                                aux_line_5_2 = Line::new(PlotPoints::default());
                                line_2 = Line::new(PlotPoints::default());
                            }
                        }

                        let scroll: usize = (sample_rate.load(Ordering::Relaxed) as usize / 1000.0 as usize) * params.scrollspeed.value() as usize;
                        samples.update_internal_length(scroll);
                        samples_2.update_internal_length(scroll);

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
                                &en_filled_lines,
                                "Filled Lines",
                            ));
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

                let mut final_primary_color: Color32 = Default::default();
                let mut final_aux_line_color: Color32 = Default::default();
                let mut final_aux_line_color_2: Color32 = Default::default();
                let mut final_aux_line_color_3: Color32 = Default::default();
                let mut final_aux_line_color_4: Color32 = Default::default();
                let mut final_aux_line_color_5: Color32 = Default::default();
                ui.allocate_ui(egui::Vec2::new(900.0, 380.0), |ui| {
                    // Fix our colors to focus on our line
                    match ontop.load(Ordering::SeqCst) {
                        0 => {
                            // Main unaffected
                            final_primary_color = primary_line_color;
                            final_aux_line_color = soft_aux_1;
                            final_aux_line_color_2 = soft_aux_2;
                            final_aux_line_color_3 = soft_aux_3;
                            final_aux_line_color_4 = soft_aux_4;
                            final_aux_line_color_5 = soft_aux_5;
                        }
                        1 => {
                            // aux unaffected
                            final_primary_color = soft_primary;
                            final_aux_line_color = user_aux_1;
                            final_aux_line_color_2 = soft_aux_2;
                            final_aux_line_color_3 = soft_aux_3;
                            final_aux_line_color_4 = soft_aux_4;
                            final_aux_line_color_5 = soft_aux_5;
                        }
                        2 => {
                            // aux 2 unaffected
                            final_primary_color = soft_primary;
                            final_aux_line_color = soft_aux_1;
                            final_aux_line_color_2 = user_aux_2;
                            final_aux_line_color_3 = soft_aux_3;
                            final_aux_line_color_4 = soft_aux_4;
                            final_aux_line_color_5 = soft_aux_5;
                        }
                        3 => {
                            // aux 3 unaffected
                            final_primary_color = soft_primary;
                            final_aux_line_color = soft_aux_1;
                            final_aux_line_color_2 = soft_aux_2;
                            final_aux_line_color_3 = user_aux_3;
                            final_aux_line_color_4 = soft_aux_4;
                            final_aux_line_color_5 = soft_aux_5;
                        }
                        4 => {
                            // aux 4 unaffected
                            final_primary_color = soft_primary;
                            final_aux_line_color = soft_aux_1;
                            final_aux_line_color_2 = soft_aux_2;
                            final_aux_line_color_3 = soft_aux_3;
                            final_aux_line_color_4 = user_aux_4;
                            final_aux_line_color_5 = soft_aux_5;
                        }
                        5 => {
                            // aux 5 unaffected
                            final_primary_color = soft_primary;
                            final_aux_line_color = soft_aux_1;
                            final_aux_line_color_2 = soft_aux_2;
                            final_aux_line_color_3 = soft_aux_3;
                            final_aux_line_color_4 = soft_aux_4;
                            final_aux_line_color_5 = user_aux_5;
                        }
                        _ => {
                            // We shouldn't be here
                        }
                    }
                    // Show the frequency analyzer
                    if show_analyzer.load(Ordering::SeqCst) {
                        let mut shapes: Vec<egui::Shape> = vec![];
                        let t_sr = sr.load(Ordering::Relaxed);
                        let scroll: usize = (t_sr as usize / 1000.0 as usize) * params.scrollspeed.value() as usize;
                        // Sample Buffer ONE calculations
                        // Compute our fast fourier transforms
                        let mut buffer: Vec<Complex<f32>> = samples.get_complex_samples_with_length(0, scroll);
                        let buffer_len: usize = buffer.len();
                        let fft_plan: Arc<dyn Fft<f32>> = fft.lock().unwrap().plan_fft(buffer_len, FftDirection::Forward);
                        fft_plan.process(&mut buffer);
                        let mut ax1: Vec<Complex<f32>> = samples.get_complex_samples_with_length(1, scroll);
                        let ax1_len: usize = ax1.len();
                        let fft_plan: Arc<dyn Fft<f32>> = fft.lock().unwrap().plan_fft(ax1_len, FftDirection::Forward);
                        fft_plan.process(&mut ax1);
                        let mut ax2: Vec<Complex<f32>> = samples.get_complex_samples_with_length(2, scroll);
                        let ax2_len: usize = ax2.len();
                        let fft_plan: Arc<dyn Fft<f32>> = fft.lock().unwrap().plan_fft(ax2_len, FftDirection::Forward);
                        fft_plan.process(&mut ax2);
                        let mut ax3: Vec<Complex<f32>> = samples.get_complex_samples_with_length(3, scroll);
                        let ax3_len: usize = ax3.len();
                        let fft_plan: Arc<dyn Fft<f32>> = fft.lock().unwrap().plan_fft(ax3_len, FftDirection::Forward);
                        fft_plan.process(&mut ax3);
                        let mut ax4: Vec<Complex<f32>> = samples.get_complex_samples_with_length(4, scroll);
                        let ax4_len: usize = ax4.len();
                        let fft_plan: Arc<dyn Fft<f32>> = fft.lock().unwrap().plan_fft(ax4_len, FftDirection::Forward);
                        fft_plan.process(&mut ax4);
                        let mut ax5: Vec<Complex<f32>> = samples.get_complex_samples_with_length(5, scroll);
                        let ax5_len: usize = ax5.len();
                        let fft_plan: Arc<dyn Fft<f32>> = fft.lock().unwrap().plan_fft(ax5_len, FftDirection::Forward);
                        fft_plan.process(&mut ax5);
                        // Compute
                        let magnitudes: Vec<f32> = buffer.iter().map(|c| c.norm() as f32).collect();
                        let frequencies: Vec<f32> = (0..buffer_len / 2)
                            .map(|i| i as f32 * t_sr / buffer_len as f32)
                            .collect();
                        let magnitudes_ax1: Vec<f32> = ax1.iter().map(|c| c.norm() as f32).collect();
                        let frequencies_ax1: Vec<f32> = (0..ax1_len / 2)
                            .map(|i| i as f32 * t_sr / ax1_len as f32)
                            .collect();
                        let magnitudes_ax2: Vec<f32> = ax2.iter().map(|c| c.norm() as f32).collect();
                        let frequencies_ax2: Vec<f32> = (0..ax2_len / 2)
                            .map(|i| i as f32 * t_sr / ax2_len as f32)
                            .collect();
                        let magnitudes_ax3: Vec<f32> = ax3.iter().map(|c| c.norm() as f32).collect();
                        let frequencies_ax3: Vec<f32> = (0..ax3_len / 2)
                            .map(|i| i as f32 * t_sr / ax3_len as f32)
                            .collect();
                        let magnitudes_ax4: Vec<f32> = ax4.iter().map(|c| c.norm() as f32).collect();
                        let frequencies_ax4: Vec<f32> = (0..ax4_len / 2)
                            .map(|i| i as f32 * t_sr / ax4_len as f32)
                            .collect();
                        let magnitudes_ax5: Vec<f32> = ax5.iter().map(|c| c.norm() as f32).collect();
                        let frequencies_ax5: Vec<f32> = (0..ax5_len / 2)
                            .map(|i| i as f32 * t_sr / ax5_len as f32)
                            .collect();
                        // Scale for visibility
                        let db_scaler: f32 = 2.75;
                        let freq_scaler: f32 = 285.0;
                        let x_shift: f32 = -220.0;
                        let y_shift: f32 = 220.0;
                        // 1Khz pivot and -4.5 slope is same as Fruity Parametric EQ2
                        // For some reason 12 lines up the same here...
                        let pivot: f32 = 1000.0;
                        let slope: f32 = 12.0;
                        if en_bar_mode.load(Ordering::SeqCst) {
                            let length = frequencies.len();
                            //let bar_scaler = 300.0;
                            let bar_scaler: f32 = 1.6;
                            let bars: f32 = 64.0;
                            let chunk_size: f32 = length as f32 / bars;
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                                        CornerRadius::ZERO,
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
                                    if en_filled_lines.load(Ordering::SeqCst) {
                                        if en_aux5.load(Ordering::SeqCst) { 
                                            for point in ax5_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_5
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux4.load(Ordering::SeqCst) { 
                                            for point in ax4_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_4
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux3.load(Ordering::SeqCst) { 
                                            for point in ax3_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_3
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux2.load(Ordering::SeqCst) { 
                                            for point in ax2_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_2
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux1.load(Ordering::SeqCst) { 
                                            for point in ax1_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color
                                                    )
                                                );
                                            }    
                                        }
                                        if en_main.load(Ordering::SeqCst) {
                                            for point in data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_primary_color
                                                    )
                                                );
                                            }
                                        }
                                    } else {
                                        if en_aux5.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax5_data, Stroke::new(1.0, final_aux_line_color_5))); }
                                        if en_aux4.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax4_data, Stroke::new(1.0, final_aux_line_color_4))); }
                                        if en_aux3.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax3_data, Stroke::new(1.0, final_aux_line_color_3))); }
                                        if en_aux2.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax2_data, Stroke::new(1.0, final_aux_line_color_2))); }
                                        if en_aux1.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax1_data, Stroke::new(1.0, final_aux_line_color))); }
                                        if en_main.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(data, Stroke::new(1.0, final_primary_color))); }
                                    }
                                }
                                1 => {
                                    if en_filled_lines.load(Ordering::SeqCst) {
                                        if en_main.load(Ordering::SeqCst) {
                                            for point in data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_primary_color
                                                    )
                                                );
                                            }
                                        }
                                        if en_aux5.load(Ordering::SeqCst) { 
                                            for point in ax5_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_5
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux4.load(Ordering::SeqCst) { 
                                            for point in ax4_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_4
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux3.load(Ordering::SeqCst) { 
                                            for point in ax3_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_3
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux2.load(Ordering::SeqCst) { 
                                            for point in ax2_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_2
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux1.load(Ordering::SeqCst) { 
                                            for point in ax1_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color
                                                    )
                                                );
                                            }    
                                        }
                                    } else {
                                        if en_main.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(data, Stroke::new(1.0, final_primary_color))); }
                                        if en_aux5.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax5_data, Stroke::new(1.0, final_aux_line_color_5))); }
                                        if en_aux4.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax4_data, Stroke::new(1.0, final_aux_line_color_4))); }
                                        if en_aux3.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax3_data, Stroke::new(1.0, final_aux_line_color_3))); }
                                        if en_aux2.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax2_data, Stroke::new(1.0, final_aux_line_color_2))); }
                                        if en_aux1.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax1_data, Stroke::new(1.0, final_aux_line_color))); }
                                    }
                                }
                                2 => {
                                    if en_filled_lines.load(Ordering::SeqCst) {
                                        if en_aux1.load(Ordering::SeqCst) { 
                                            for point in ax1_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color
                                                    )
                                                );
                                            }    
                                        }
                                        if en_main.load(Ordering::SeqCst) {
                                            for point in data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_primary_color
                                                    )
                                                );
                                            }
                                        }
                                        if en_aux5.load(Ordering::SeqCst) { 
                                            for point in ax5_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_5
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux4.load(Ordering::SeqCst) { 
                                            for point in ax4_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_4
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux3.load(Ordering::SeqCst) { 
                                            for point in ax3_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_3
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux2.load(Ordering::SeqCst) { 
                                            for point in ax2_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_2
                                                    )
                                                );
                                            }  
                                        }
                                    } else {
                                        if en_aux1.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax1_data, Stroke::new(1.0, final_aux_line_color))); }
                                        if en_main.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(data, Stroke::new(1.0, final_primary_color))); }
                                        if en_aux5.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax5_data, Stroke::new(1.0, final_aux_line_color_5))); }
                                        if en_aux4.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax4_data, Stroke::new(1.0, final_aux_line_color_4))); }
                                        if en_aux3.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax3_data, Stroke::new(1.0, final_aux_line_color_3))); }
                                        if en_aux2.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax2_data, Stroke::new(1.0, final_aux_line_color_2))); }
                                    }
                                }
                                3 => {
                                    if en_filled_lines.load(Ordering::SeqCst) {
                                        if en_aux2.load(Ordering::SeqCst) { 
                                            for point in ax2_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_2
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux1.load(Ordering::SeqCst) { 
                                            for point in ax1_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color
                                                    )
                                                );
                                            }    
                                        }
                                        if en_main.load(Ordering::SeqCst) {
                                            for point in data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_primary_color
                                                    )
                                                );
                                            }
                                        }
                                        if en_aux5.load(Ordering::SeqCst) { 
                                            for point in ax5_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_5
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux4.load(Ordering::SeqCst) { 
                                            for point in ax4_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_4
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux3.load(Ordering::SeqCst) { 
                                            for point in ax3_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_3
                                                    )
                                                );
                                            }  
                                        }
                                    } else {
                                        if en_aux2.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax2_data, Stroke::new(1.0, final_aux_line_color_2))); }
                                        if en_aux1.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax1_data, Stroke::new(1.0, final_aux_line_color))); }
                                        if en_main.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(data, Stroke::new(1.0, final_primary_color))); }
                                        if en_aux5.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax5_data, Stroke::new(1.0, final_aux_line_color_5))); }
                                        if en_aux4.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax4_data, Stroke::new(1.0, final_aux_line_color_4))); }
                                        if en_aux3.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax3_data, Stroke::new(1.0, final_aux_line_color_3))); }
                                    }
                                }
                                4 => {
                                    if en_filled_lines.load(Ordering::SeqCst) {
                                        if en_aux3.load(Ordering::SeqCst) { 
                                            for point in ax3_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_3
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux2.load(Ordering::SeqCst) { 
                                            for point in ax2_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_2
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux1.load(Ordering::SeqCst) { 
                                            for point in ax1_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color
                                                    )
                                                );
                                            }    
                                        }
                                        if en_main.load(Ordering::SeqCst) {
                                            for point in data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_primary_color
                                                    )
                                                );
                                            }
                                        }
                                        if en_aux5.load(Ordering::SeqCst) { 
                                            for point in ax5_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_5
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux4.load(Ordering::SeqCst) { 
                                            for point in ax4_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_4
                                                    )
                                                );
                                            }  
                                        }
                                    } else {
                                        if en_aux3.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax3_data, Stroke::new(1.0, final_aux_line_color_3))); }
                                        if en_aux2.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax2_data, Stroke::new(1.0, final_aux_line_color_2))); }
                                        if en_aux1.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax1_data, Stroke::new(1.0, final_aux_line_color))); }
                                        if en_main.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(data, Stroke::new(1.0, final_primary_color))); }
                                        if en_aux5.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax5_data, Stroke::new(1.0, final_aux_line_color_5))); }
                                        if en_aux4.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax4_data, Stroke::new(1.0, final_aux_line_color_4))); }
                                    }
                                }
                                5 => {
                                    if en_filled_lines.load(Ordering::SeqCst) {
                                        if en_aux4.load(Ordering::SeqCst) { 
                                            for point in ax4_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_4
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux3.load(Ordering::SeqCst) { 
                                            for point in ax3_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_3
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux2.load(Ordering::SeqCst) { 
                                            for point in ax2_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_2
                                                    )
                                                );
                                            }  
                                        }
                                        if en_aux1.load(Ordering::SeqCst) { 
                                            for point in ax1_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color
                                                    )
                                                );
                                            }    
                                        }
                                        if en_main.load(Ordering::SeqCst) {
                                            for point in data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_primary_color
                                                    )
                                                );
                                            }
                                        }
                                        if en_aux5.load(Ordering::SeqCst) { 
                                            for point in ax5_data.iter() {
                                                shapes.push(
                                                    epaint::Shape::rect_filled(
                                                        Rect { 
                                                            min: Pos2::new(point.x, point.y), 
                                                            max: Pos2::new(point.x + 0.5, 500.0)
                                                        },
                                                        CornerRadius::ZERO,
                                                        final_aux_line_color_5
                                                    )
                                                );
                                            }  
                                        }
                                    } else {
                                        if en_aux4.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax4_data, Stroke::new(1.0, final_aux_line_color_4))); }
                                        if en_aux3.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax3_data, Stroke::new(1.0, final_aux_line_color_3))); }
                                        if en_aux2.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax2_data, Stroke::new(1.0, final_aux_line_color_2))); }
                                        if en_aux1.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax1_data, Stroke::new(1.0, final_aux_line_color))); }
                                        if en_main.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(data, Stroke::new(1.0, final_primary_color))); }
                                        if en_aux5.load(Ordering::SeqCst) { shapes.push(epaint::Shape::line(ax5_data, Stroke::new(1.0, final_aux_line_color_5))); }
                                    }
                                }
                                _ => {
                                    // We shouldn't be here
                                }
                            }
                            ui.painter().extend(shapes);
                        }
                    } else {
                        //let internal_length = samples.internal_length.load(Ordering::SeqCst);
                        //let internal_length_2 = samples_2.internal_length.load(Ordering::SeqCst);
                        //let write_indices = samples.write_indices[0].load(Ordering::SeqCst);
                        let sbl: PlotPoints = {
                            // Get a read lock on the buffer
                            let buffer_len = samples.internal_length.load(Ordering::Acquire);
                            let main_samples = samples.get_samples(6); // Now returns Vec<f32> directly

                            (0..buffer_len)
                                .map(|i| {
                                    let x = i as f64; // Linear index
                                    let y = main_samples[i] as f64;
                                    [x, y]
                                })
                                .collect()
                        };
                        let sbl_line = Line::new(sbl)
                            .color(guidelines)
                            .stroke(Stroke::new(0.25, guidelines.linear_multiply(0.5)));
                        let offset_osc_view;
                        if stereo_view.load(Ordering::Relaxed) {
                            offset_osc_view = 1.0;
                        } else {
                            offset_osc_view = 0.0;
                        }
                        // CHANNEL 0
                        /////////////////////////////////////////////////////////////////////////////////////////
                        // Primary Input
                        // Primary Input
                        let data: PlotPoints = {
                            // Get a read lock on the buffer
                            let buffer_len = samples.internal_length.load(Ordering::Acquire);
                            let main_samples = samples.get_samples(0); // Now returns Vec<f32> directly

                            (0..buffer_len)
                                .map(|i| {
                                    let x = i as f64; // Linear index
                                    let y = if en_main.load(Ordering::Relaxed) {
                                        main_samples[i] as f64 + offset_osc_view
                                    } else {
                                        0.0
                                    };
                                    [x, y]
                                })
                                .collect()
                        };
                        line = Line::new(data)
                            .color(primary_line_color)
                            .stroke(Stroke::new(1.1, primary_line_color));
                        // Aux inputs
                        let aux_data: PlotPoints = {
                            let buffer_len = samples.internal_length.load(Ordering::Acquire);
                            let aux1_samples = samples.get_samples(1); // Channel 1 for Aux1

                            (0..buffer_len)
                                .map(|i| {
                                    let x = i as f64;
                                    let y = if en_aux1.load(Ordering::Relaxed) {
                                        aux1_samples[i] as f64 + offset_osc_view
                                    } else {
                                        0.0
                                    };
                                    [x, y]
                                })
                                .collect()
                        };
                        aux_line = Line::new(aux_data)
                            .color(user_aux_1)
                            .stroke(Stroke::new(1.0, user_aux_1));
                        let aux_data_2: PlotPoints = {
                            let buffer_len = samples.internal_length.load(Ordering::Acquire);
                            let aux2_samples = samples.get_samples(2); // Channel 2 for Aux2
                                                
                            (0..buffer_len)
                                .map(|i| {
                                    let x = i as f64;
                                    let y = if en_aux2.load(Ordering::Relaxed) {
                                        aux2_samples[i] as f64 + offset_osc_view
                                    } else {
                                        0.0
                                    };
                                    [x, y]
                                })
                                .collect()
                        };
                        aux_line_2 = Line::new(aux_data_2)
                            .color(user_aux_2)
                            .stroke(Stroke::new(1.0, user_aux_2));
                        let aux_data_3: PlotPoints = {
                            let buffer_len = samples.internal_length.load(Ordering::Acquire);
                            let aux3_samples = samples.get_samples(3); // Channel 3 for Aux3
                                                
                            (0..buffer_len)
                                .map(|i| {
                                    let x = i as f64;
                                    let y = if en_aux3.load(Ordering::Relaxed) {
                                        aux3_samples[i] as f64 + offset_osc_view
                                    } else {
                                        0.0
                                    };
                                    [x, y]
                                })
                                .collect()
                        };
                        aux_line_3 = Line::new(aux_data_3)
                            .color(user_aux_3)
                            .stroke(Stroke::new(1.0, user_aux_3));
                        let aux_data_4: PlotPoints = {
                            let buffer_len = samples.internal_length.load(Ordering::Acquire);
                            let aux4_samples = samples.get_samples(4); // Channel 4 for Aux4
                                                
                            (0..buffer_len)
                                .map(|i| {
                                    let x = i as f64;
                                    let y = if en_aux4.load(Ordering::Relaxed) {
                                        aux4_samples[i] as f64 + offset_osc_view
                                    } else {
                                        0.0
                                    };
                                    [x, y]
                                })
                                .collect()
                        };
                        aux_line_4 = Line::new(aux_data_4)
                            .color(user_aux_4)
                            .stroke(Stroke::new(1.0, user_aux_4));
                        let aux_data_5: PlotPoints = {
                            let buffer_len = samples.internal_length.load(Ordering::Acquire);
                            let aux5_samples = samples.get_samples(5); // Channel 5 for Aux5
                                                
                            (0..buffer_len)
                                .map(|i| {
                                    let x = i as f64;
                                    let y = if en_aux5.load(Ordering::Relaxed) {
                                        aux5_samples[i] as f64 + offset_osc_view
                                    } else {
                                        0.0
                                    };
                                    [x, y]
                                })
                                .collect()
                        };
                        aux_line_5 = Line::new(aux_data_5)
                            .color(user_aux_5)
                            .stroke(Stroke::new(1.0, user_aux_5));
                        let sum_plotpoints: PlotPoints = {
                            let buffer_len = samples.internal_length.load(Ordering::Acquire);
                            let sum_samples = samples.get_samples(7); // Channel 7 for sum
                                                
                            (0..buffer_len)
                                .map(|i| {
                                    let x = i as f64;
                                    let y = if en_sum.load(Ordering::Relaxed) {
                                        sum_samples[i] as f64 + offset_osc_view
                                    } else {
                                        0.0
                                    };
                                    [x, y]
                                })
                                .collect()
                        };
                        sum_line = Line::new(sum_plotpoints)
                                .color(user_sum_line.linear_multiply(0.25))
                                .stroke(Stroke::new(0.9, user_sum_line));

                        // CHANNEL 1
                        /////////////////////////////////////////////////////////////////////////////////////////
                        // Primary Input
                        let data_2: PlotPoints = {
                            // Get a read lock on the buffer
                            let buffer_len = samples_2.internal_length.load(Ordering::Acquire);
                            let main_samples = samples_2.get_samples(0); // Now returns Vec<f32> directly

                            (0..buffer_len)
                                .map(|i| {
                                    let x = i as f64; // Linear index
                                    let y = if en_main.load(Ordering::Relaxed) {
                                        main_samples[i] as f64 + offset_osc_view
                                    } else {
                                        0.0
                                    };
                                    [x, y]
                                })
                                .collect()
                        };
                        line_2 = Line::new(data_2)
                            .color(primary_line_color)
                            .stroke(Stroke::new(1.1, primary_line_color));
                        // Aux inputs
                        #[allow(non_snake_case)]
                        let aux_data__2: PlotPoints = {
                            let buffer_len = samples_2.internal_length.load(Ordering::Acquire);
                            let aux1_samples = samples_2.get_samples(1); // Channel 1 for Aux1

                            (0..buffer_len)
                                .map(|i| {
                                    let x = i as f64;
                                    let y = if en_aux1.load(Ordering::Relaxed) {
                                        aux1_samples[i] as f64 + offset_osc_view
                                    } else {
                                        0.0
                                    };
                                    [x, y]
                                })
                                .collect()
                        };
                        aux_line__2 = Line::new(aux_data__2)
                            .color(user_aux_1)
                            .stroke(Stroke::new(1.0, user_aux_1));
                        let aux_data_2_2: PlotPoints = {
                            let buffer_len = samples_2.internal_length.load(Ordering::Acquire);
                            let aux2_samples = samples_2.get_samples(2); // Channel 2 for Aux2
                                                
                            (0..buffer_len)
                                .map(|i| {
                                    let x = i as f64;
                                    let y = if en_aux2.load(Ordering::Relaxed) {
                                        aux2_samples[i] as f64 + offset_osc_view
                                    } else {
                                        0.0
                                    };
                                    [x, y]
                                })
                                .collect()
                        };
                        aux_line_2_2 = Line::new(aux_data_2_2)
                            .color(user_aux_2)
                            .stroke(Stroke::new(1.0, user_aux_2));
                        let aux_data_3_2: PlotPoints = {
                            let buffer_len = samples_2.internal_length.load(Ordering::Acquire);
                            let aux3_samples = samples_2.get_samples(3); // Channel 3 for Aux3
                                                
                            (0..buffer_len)
                                .map(|i| {
                                    let x = i as f64;
                                    let y = if en_aux3.load(Ordering::Relaxed) {
                                        aux3_samples[i] as f64 + offset_osc_view
                                    } else {
                                        0.0
                                    };
                                    [x, y]
                                })
                                .collect()
                        };
                        aux_line_3_2 = Line::new(aux_data_3_2)
                            .color(user_aux_3)
                            .stroke(Stroke::new(1.0, user_aux_3));
                        let aux_data_4_2: PlotPoints = {
                            let buffer_len = samples_2.internal_length.load(Ordering::Acquire);
                            let aux4_samples = samples_2.get_samples(4); // Channel 4 for Aux4
                                                
                            (0..buffer_len)
                                .map(|i| {
                                    let x = i as f64;
                                    let y = if en_aux4.load(Ordering::Relaxed) {
                                        aux4_samples[i] as f64 + offset_osc_view
                                    } else {
                                        0.0
                                    };
                                    [x, y]
                                })
                                .collect()
                        };
                        aux_line_4_2 = Line::new(aux_data_4_2)
                            .color(user_aux_4)
                            .stroke(Stroke::new(1.0, user_aux_4));
                        let aux_data_5_2: PlotPoints = {
                            let buffer_len = samples_2.internal_length.load(Ordering::Acquire);
                            let aux5_samples = samples_2.get_samples(5); // Channel 5 for Aux5
                                                
                            (0..buffer_len)
                                .map(|i| {
                                    let x = i as f64;
                                    let y = if en_aux5.load(Ordering::Relaxed) {
                                        aux5_samples[i] as f64 + offset_osc_view
                                    } else {
                                        0.0
                                    };
                                    [x, y]
                                })
                                .collect()
                        };
                        aux_line_5_2 = Line::new(aux_data_5_2)
                            .color(user_aux_5)
                            .stroke(Stroke::new(1.0, user_aux_5));
                        let sum_plotpoints_2: PlotPoints = {
                            let buffer_len = samples_2.internal_length.load(Ordering::Acquire);
                            let sum_samples = samples_2.get_samples(7); // Channel 7 for sum
                                                
                            (0..buffer_len)
                                .map(|i| {
                                    let x = i as f64;
                                    let y = if en_sum.load(Ordering::Relaxed) {
                                        sum_samples[i] as f64 + offset_osc_view
                                    } else {
                                        0.0
                                    };
                                    [x, y]
                                })
                                .collect()
                        };
                        let sum_line_2 = Line::new(sum_plotpoints_2)
                                .color(user_sum_line.linear_multiply(0.25))
                                .stroke(Stroke::new(0.9, user_sum_line));
                        // Show the Oscilloscope
                        Plot::new("Oscilloscope")
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
                            .x_grid_spacer(move |_| {
                                vec![
                                   // 100s
                                   //GridMark { value: 100.0, step_size: 100.0 },
                                ]
                            })
                            // Format hover to blank or value
                            .label_formatter(|_, _| "".to_owned())
                            .show(ui, |plot_ui| {
                                plot_ui.line(sbl_line);
                                if en_sum.load(Ordering::SeqCst) {
                                    // Draw the sum line first so it's furthest behind
                                    if en_filled_osc.load(Ordering::SeqCst) {
                                        plot_ui.line(sum_line.fill(0.0));
                                        plot_ui.line(sum_line_2.fill(0.0));
                                    } else {
                                        plot_ui.line(sum_line);
                                        plot_ui.line(sum_line_2);
                                    }
                                }
                                // Figure out the lines to draw
                                // Get our fill for this sequence
                                let fill = en_filled_osc.load(Ordering::SeqCst);
                                // Draw whichever order next
                                match ontop.load(Ordering::SeqCst) {
                                    0 => {
                                        if en_aux5.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_5.fill(0.0));
                                                plot_ui.line(aux_line_5_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_5);
                                                plot_ui.line(aux_line_5_2);
                                            }
                                        }
                                        if en_aux4.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_4.fill(0.0));
                                                plot_ui.line(aux_line_4_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_4);
                                                plot_ui.line(aux_line_4_2);
                                            }
                                        }
                                        if en_aux3.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_3.fill(0.0));
                                                plot_ui.line(aux_line_3_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_3);
                                                plot_ui.line(aux_line_3_2);
                                            }
                                        }
                                        if en_aux2.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_2.fill(0.0));
                                                plot_ui.line(aux_line_2_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_2);
                                                plot_ui.line(aux_line_2_2);
                                            }
                                        }
                                        if en_aux1.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line.fill(0.0));
                                                plot_ui.line(aux_line__2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line);
                                                plot_ui.line(aux_line__2);
                                            }
                                        }
                                        if en_main.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(line.fill(0.0));
                                                plot_ui.line(line_2.fill(0.0));
                                            } else {
                                                plot_ui.line(line);
                                                plot_ui.line(line_2);
                                            }
                                        }
                                    }
                                    1 => {
                                        if en_main.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(line.fill(0.0));
                                                plot_ui.line(line_2.fill(0.0));
                                            } else {
                                                plot_ui.line(line);
                                                plot_ui.line(line_2);
                                            }
                                        }
                                        if en_aux5.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_5.fill(0.0));
                                                plot_ui.line(aux_line_5_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_5);
                                                plot_ui.line(aux_line_5_2);
                                            }
                                        }
                                        if en_aux4.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_4.fill(0.0));
                                                plot_ui.line(aux_line_4_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_4);
                                                plot_ui.line(aux_line_4_2);
                                            }
                                        }
                                        if en_aux3.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_3.fill(0.0));
                                                plot_ui.line(aux_line_3_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_3);
                                                plot_ui.line(aux_line_3_2);
                                            }
                                        }
                                        if en_aux2.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_2.fill(0.0));
                                                plot_ui.line(aux_line_2_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_2);
                                                plot_ui.line(aux_line_2_2);
                                            }
                                        }
                                        if en_aux1.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line.fill(0.0));
                                                plot_ui.line(aux_line__2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line);
                                                plot_ui.line(aux_line__2);
                                            }
                                        }
                                    }
                                    2 => {
                                        if en_aux1.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line.fill(0.0));
                                                plot_ui.line(aux_line__2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line);
                                                plot_ui.line(aux_line__2);
                                            }
                                        }
                                        if en_main.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(line.fill(0.0));
                                                plot_ui.line(line_2.fill(0.0));
                                            } else {
                                                plot_ui.line(line);
                                                plot_ui.line(line_2);
                                            }
                                        }
                                        if en_aux5.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_5.fill(0.0));
                                                plot_ui.line(aux_line_5_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_5);
                                                plot_ui.line(aux_line_5_2);
                                            }
                                        }
                                        if en_aux4.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_4.fill(0.0));
                                                plot_ui.line(aux_line_4_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_4);
                                                plot_ui.line(aux_line_4_2);
                                            }
                                        }
                                        if en_aux3.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_3.fill(0.0));
                                                plot_ui.line(aux_line_3_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_3);
                                                plot_ui.line(aux_line_3_2);
                                            }
                                        }
                                        if en_aux2.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_2.fill(0.0));
                                                plot_ui.line(aux_line_2_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_2);
                                                plot_ui.line(aux_line_2_2);
                                            }
                                        }
                                    }
                                    3 => {
                                        if en_aux2.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_2.fill(0.0));
                                                plot_ui.line(aux_line_2_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_2);
                                                plot_ui.line(aux_line_2_2);
                                            }
                                        }
                                        if en_aux1.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line.fill(0.0));
                                                plot_ui.line(aux_line__2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line);
                                                plot_ui.line(aux_line__2);
                                            }
                                        }
                                        if en_main.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(line.fill(0.0));
                                                plot_ui.line(line_2.fill(0.0));
                                            } else {
                                                plot_ui.line(line);
                                                plot_ui.line(line_2);
                                            }
                                        }
                                        if en_aux5.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_5.fill(0.0));
                                                plot_ui.line(aux_line_5_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_5);
                                                plot_ui.line(aux_line_5_2);
                                            }
                                        }
                                        if en_aux4.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_4.fill(0.0));
                                                plot_ui.line(aux_line_4_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_4);
                                                plot_ui.line(aux_line_4_2);
                                            }
                                        }
                                        if en_aux3.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_3.fill(0.0));
                                                plot_ui.line(aux_line_3_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_3);
                                                plot_ui.line(aux_line_3_2);
                                            }
                                        }
                                    }
                                    4 => {
                                        if en_aux3.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_3.fill(0.0));
                                                plot_ui.line(aux_line_3_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_3);
                                                plot_ui.line(aux_line_3_2);
                                            }
                                        }
                                        if en_aux2.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_2.fill(0.0));
                                                plot_ui.line(aux_line_2_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_2);
                                                plot_ui.line(aux_line_2_2);
                                            }
                                        }
                                        if en_aux1.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line.fill(0.0));
                                                plot_ui.line(aux_line__2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line);
                                                plot_ui.line(aux_line__2);
                                            }
                                        }
                                        if en_main.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(line.fill(0.0));
                                                plot_ui.line(line_2.fill(0.0));
                                            } else {
                                                plot_ui.line(line);
                                                plot_ui.line(line_2);
                                            }
                                        }
                                        if en_aux5.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_5.fill(0.0));
                                                plot_ui.line(aux_line_5_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_5);
                                                plot_ui.line(aux_line_5_2);
                                            }
                                        }
                                        if en_aux4.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_4.fill(0.0));
                                                plot_ui.line(aux_line_4_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_4);
                                                plot_ui.line(aux_line_4_2);
                                            }
                                        }
                                    }
                                    5 => {
                                        if en_aux4.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_4.fill(0.0));
                                                plot_ui.line(aux_line_4_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_4);
                                                plot_ui.line(aux_line_4_2);
                                            }
                                        }
                                        if en_aux3.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_3.fill(0.0));
                                                plot_ui.line(aux_line_3_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_3);
                                                plot_ui.line(aux_line_3_2);
                                            }
                                        }
                                        if en_aux2.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_2.fill(0.0));
                                                plot_ui.line(aux_line_2_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_2);
                                                plot_ui.line(aux_line_2_2);
                                            }
                                        }
                                        if en_aux1.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line.fill(0.0));
                                                plot_ui.line(aux_line__2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line);
                                                plot_ui.line(aux_line__2);
                                            }
                                        }
                                        if en_main.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(line.fill(0.0));
                                                plot_ui.line(line_2.fill(0.0));
                                            } else {
                                                plot_ui.line(line);
                                                plot_ui.line(line_2);
                                            }
                                        }
                                        if en_aux5.load(Ordering::SeqCst) {
                                            if fill {
                                                plot_ui.line(aux_line_5.fill(0.0));
                                                plot_ui.line(aux_line_5_2.fill(0.0));
                                            } else {
                                                plot_ui.line(aux_line_5);
                                                plot_ui.line(aux_line_5_2);
                                            }
                                        }
                                    }
                                    _ => {
                                        // We shouldn't be here
                                    }
                                }
                                // Draw our clipping guides if needed
                                let clip_counter = is_clipping.load(Ordering::Relaxed);
                                if clip_counter > 0.0 {
                                    if stereo_view.load(Ordering::SeqCst) {
                                        plot_ui.hline(
                                            HLine::new(2.0)
                                                .color(egui::Color32::RED)
                                                .stroke(Stroke::new(0.6, Color32::RED)),
                                        );
                                        plot_ui.hline(
                                            HLine::new(-2.0)
                                                .color(Color32::RED)
                                                .stroke(Stroke::new(0.6, Color32::RED)),
                                        );
                                        plot_ui.hline(
                                            HLine::new(0.0)
                                                .color(Color32::RED)
                                                .stroke(Stroke::new(0.6, Color32::RED)),
                                        );
                                    } else {
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
                                    }
                                    is_clipping.store(clip_counter - 1.0, Ordering::Relaxed);
                                }
                            })
                        .response;
                    }
                });
                // Floating buttons
                if !show_analyzer.load(Ordering::Relaxed) {
                    let mut stereo_switch_ui = ui.new_child(UiBuilder::new().max_rect(Rect { min: Pos2 { x: 740.0, y: 30.0 }, max: Pos2 { x: 1040.0, y: 40.0 } }));
                    stereo_switch_ui
                        .scope(|ui| {
                            ui.horizontal(|ui|{
                                let checkstereo = slim_checkbox::AtomicSlimCheckbox::new(&stereo_view, "Stereo View");
                                ui.add(checkstereo);
                                let leftchannel = slim_checkbox::AtomicSlimCheckbox::new(&stereo_view, "Left Channel");
                                ui.add(leftchannel);
                                let rightchanel = slim_checkbox::AtomicSlimCheckbox::new(&stereo_view, "Right Channel");
                                ui.add(rightchanel);
                            });
                        }).inner;
                }
            });
        },
    )
}
