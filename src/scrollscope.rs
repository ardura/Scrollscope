/*******************************************************
 * Scrollscope Oscilloscpe for use with EGUI by Ardura
 *******************************************************/
 
use atomic_float::AtomicF32;
use nih_plug_egui::egui::Color32;
use nih_plug_egui::egui::Pos2;
use nih_plug_egui::egui::Rect;
use nih_plug_egui::egui::Response;
use nih_plug_egui::egui::Rgba;
use nih_plug_egui::egui::Sense;
use nih_plug_egui::egui::Shape;
use nih_plug_egui::egui::Stroke;
use nih_plug_egui::egui::Ui;
use nih_plug_egui::egui::lerp;
use nih_plug_egui::egui::vec2;
use nih_plug_egui::egui::NumExt;
use egui::Vec2;
use std::sync::Arc;
use std::sync::Mutex;
use crate::egui::widgets::*;

#[must_use = "You should put this widget in an ui with `ui.add(widget);`"]
pub struct Oscilloscope {
    samples: Arc<Mutex<Vec<AtomicF32>>>,
    desired_width: Option<f32>,
    desired_height: Option<f32>,
    fill: Option<Color32>,
    x_scale: Option<f32>,
    y_scale: Option<f32>,
    scroll_x: Option<f32>,
}

impl Oscilloscope {
    // Create a new Oscilloscope that will use our custom drawing code
    pub fn new( ) -> {
        Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            width: 800.0,
            height: 320.0,
            x_scale: 1.0,
            y_scale: 1.0,
            scroll_x: Arc::new(AtomicF32::new(1.0)),
        }
    }

    /// The desired width of the bar. Will use all horizontal space if not set.
    pub fn desired_width(mut self, desired_width: f32) -> Self {
        self.desired_width = Some(desired_width);
        self
    }

    /// The fill color of the bar.
    pub fn fill(mut self, color: Color32) -> Self {
        self.fill = Some(color);
        self
    }
/*
        pub fn add_sample(&mut self, sample: f32) {
            let mut samples = self.samples.lock().unwrap();
            samples.push(sample.into());
            while samples.len() as f32 > self.width / self.x_scale {
                samples.remove(0);
            }
        }
        
        pub fn update_vals(&mut self, scroll_x: f32) {
            self.scroll_x.store(scroll_x, Ordering::Relaxed);
        }
        */
}

fn ui(self, ui: &mut Ui) -> Response {
    let ProgressBar {
        progress,
        desired_width,
        fill,
        animate,
    } = self;

    let animate = animate && progress < 1.0;

    let desired_width =
        desired_width.unwrap_or_else(|| ui.available_size_before_wrap().x.at_least(96.0));
    let height = ui.spacing().interact_size.y;
    let (outer_rect, response) =
        ui.allocate_exact_size(vec2(desired_width, height), Sense::hover());

    if ui.is_rect_visible(response.rect) {
        if animate {
            ui.ctx().request_repaint();
        }

        let visuals = ui.style().visuals.clone();
        let rounding = outer_rect.height() / 2.0;
        ui.painter()
            .rect(outer_rect, rounding, visuals.extreme_bg_color, Stroke::NONE);
        let inner_rect = Rect::from_min_size(
            outer_rect.min,
            vec2(
                (outer_rect.width() * progress).at_least(outer_rect.height()),
                outer_rect.height(),
            ),
        );

        let (dark, bright) = (0.7, 1.0);
        let color_factor = if animate {
            let time = ui.input(|i| i.time);
            lerp(dark..=bright, time.cos().abs())
        } else {
            bright
        };

        ui.painter().rect(
            inner_rect,
            rounding,
            Color32::from(
                Rgba::from(fill.unwrap_or(visuals.selection.bg_fill)) * color_factor as f32,
            ),
            Stroke::NONE,
        );

        if animate {
            let n_points = 20;
            let time = ui.input(|i| i.time);
            let start_angle = time * std::f64::consts::TAU;
            let end_angle = start_angle + 240f64.to_radians() * time.sin();
            let circle_radius = rounding - 2.0;
            let points: Vec<Pos2> = (0..n_points)
                .map(|i| {
                    let angle = lerp(start_angle..=end_angle, i as f64 / n_points as f64);
                    let (sin, cos) = angle.sin_cos();
                    inner_rect.right_center()
                        + circle_radius * vec2(cos as f32, sin as f32)
                        + vec2(-rounding, 0.0)
                })
                .collect();
            ui.painter()
                .add(Shape::line(points, Stroke::new(2.0, visuals.text_color())));
        }
    }

    response
}
/*        let samples = self.samples.lock().unwrap();
        for (i, sample) in samples.iter().enumerate() {
            let x = i as f32 * self.x_scale + self.scroll_x.load(Ordering::Relaxed);
            let y = sample.load(Ordering::Relaxed) * self.y_scale;
    
            if i == 0 {
                path.move_to(x, y);
            } else {
                path.line_to(x, y);
            }
        }
    
        canvas.stroke_path(&mut path, &paint);
*/
