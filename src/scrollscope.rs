/**************************************************
 * Scrollscope Oscilloscpe by Ardura
 **************************************************/
 
use atomic_float::AtomicF32;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;
use nih_plug_vizia::vizia::vg::{Color};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

#[derive(Default, Lens, Clone)]
pub struct Oscilloscope {
    samples: Arc<Mutex<Vec<AtomicF32>>>,
    width: f32,
    height: f32,
    x_scale: f32,
    y_scale: f32,
    scroll_x: Arc<AtomicF32>,
}

impl Oscilloscope {
    // Create a new Oscilloscope that will use our custom drawing code
    pub fn new(
        cx: &mut Context,
    ) -> Handle<Self> 
        where
        {
            Self {
                samples: Arc::new(Mutex::new(Vec::new())),
                width: 800.0,
                height: 320.0,
                x_scale: 1.0,
                y_scale: 1.0,
                scroll_x: Arc::new(AtomicF32::new(1.0)),
            }
            .build(
                cx,
                // This is empty for custom drawing according to the docs
                |_cx| (),
            )
        }

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
    }

impl View for Oscilloscope {
    fn element(&self) -> Option<&'static str> {
        Some("Oscilloscope")
    }

    //#[allow(implied_bounds_entailment)]
    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        println!("Drawing Oscilloscope");
        
        let bounds = cx.bounds();
        if bounds.w == 0.0 || bounds.h == 0.0 {
            return;
        }
    
        let line_width: f32 = cx.style.dpi_factor as f32 * 1.5;
        let paint: vg::Paint = vg::Paint::color(Color::black()).with_line_width(line_width);
        let mut path: vg::Path = vg::Path::new();
    
        let samples = self.samples.lock().unwrap();
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
    }
}
