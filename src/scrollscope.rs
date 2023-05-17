/**************************************************
 * Scrollscope Oscilloscpe by Ardura
 **************************************************/
 
 use atomic_float::AtomicF32;
 use nih_plug::nih_debug_assert;
 use nih_plug::prelude::FloatRange;
 use nih_plug_vizia::vizia::prelude::*;
 use nih_plug_vizia::vizia::vg;
 use nih_plug_vizia::vizia::vg::{Color, Canvas};
 use std::sync::atomic::Ordering;
 use std::sync::{Arc, Mutex};

#[derive(Default, Clone, Lens)]
pub struct Oscilloscope {
    samples: Arc<Vec<AtomicF32>>,
    width: f32,
    height: f32,
    x_scale: Arc<AtomicF32>,
    y_scale: f32,
    scroll_x: Arc<AtomicF32>,
}

impl Oscilloscope {
    // Create a new Oscilloscope that will use our custom drawing code
    pub fn new<Lens_x_scale, Lens_samples, Lens_scroll_x>(
        cx: &mut Context,
        x_scale: Lens_x_scale, 
        scroll_x: Lens_scroll_x,
        samples: Lens_samples
    ) -> Handle<Self> 
        where
            Lens_x_scale: Lens<Target = Arc<AtomicF32>>,
            Lens_samples: Lens<Target = Arc<Vec<AtomicF32>>>, 
            Lens_scroll_x: Lens<Target = Arc<AtomicF32>>,
        {
            Self {
                samples: samples.get(cx),
                width: 800.0,
                height: 320.0,
                x_scale: x_scale.get(cx),
                y_scale: 1.0,
                scroll_x: scroll_x.get(cx),
            }
            .build(
                cx,
                // This is empty for custom drawing according to the docs
                |_cx| (),
            )
        }

        pub fn add_sample(&mut self, sample: f32) {
            self.samples.push(sample.into());
            while self.samples.len() as f32 > self.width / self.x_scale.into_inner() {
                self.samples.remove(0);
            }
        }
        
        pub fn get_samples(&self) -> &Vec<f32> {
            &self.samples
        }
        
        pub fn get_scale(&self) -> (f32, f32) {
            (self.x_scale, self.y_scale)
        }
        
        pub fn get_scroll(&self) -> f32 {
            self.scroll_x
        }
        
        pub fn update_vals(&mut self, scroll_x: f32) {
            self.scroll_x = scroll_x;
        }

        pub fn render(&self) {
        }
    }

impl View for Oscilloscope {
    fn element(&self) -> Option<&'static str> {
        Some("oscilloscope")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas<T>) {
        let bounds = cx.bounds();
        if bounds.w == 0.0 || bounds.h == 0.0 {
            return;
        }

        let line_width = cx.style.dpi_factor as f32 * 1.5;
        let paint = vg::Paint::color(Color::black()).with_line_width(line_width);
        let mut path = vg::Path::new();

        for (i, sample) in self.samples.iter().enumerate() 
        {
            let x = i as f32 * self.x_scale.into_inner() + self.scroll_x.into_inner();
            let y = sample.into() * self.y_scale;

            path.move_to(x,y,);
            path.line_to(x ,0.0);
            
            // TODO: Figure out how to draw our line for the oscilloscope since this section is wrong
            canvas.stroke_path(path, &paint);
        }

    }



}

/*
pub fn add_sample(&mut self, sample: f32) {
    self.samples.push(sample);
    while self.samples.len() as f32 > self.width / self.x_scale {
        self.samples.remove(0);
    }
}

pub fn get_samples(&self) -> &Vec<f32> {
    &self.samples
}

pub fn get_scale(&self) -> (f32, f32) {
    (self.x_scale, self.y_scale)
}

pub fn get_scroll(&self) -> f32 {
    self.scroll_x
}

pub fn update_vals(&mut self, scroll_x: f32) {
    self.scroll_x = scroll_x;
}

pub fn render(&self) {
//pub fn render(&self, cx: &mut Context) {
    // Clear the canvas
//    cx.fill((0, 0, 0));

    // Draw the waveform
    for (i, sample) in self.samples.iter().enumerate() {
        let x = i as f32 * self.x_scale + self.scroll_x;
        let y = sample * self.y_scale;
        //cx.draw_line((x, 0), (x, y));
    }
}
*/