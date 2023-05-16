/**************************************************
 * Scrollscope Oscilloscpe by Ardura
 **************************************************/
 use nih_plug::prelude::{util, Editor};
 use nih_plug_vizia::vizia::prelude::*;

#[derive(Default, Clone, Lens)]
pub struct Oscilloscope {
    samples: Vec<f32>,
    width: f32,
    height: f32,
    x_scale: f32,
    y_scale: f32,
    scroll_x: f32,
}

impl Oscilloscope {
    pub fn new(width: f32, height: f32, x_scale: f32, y_scale: f32) -> Oscilloscope {
        Oscilloscope {
            samples: Vec::new(),
            width,
            height,
            x_scale,
            y_scale,
            scroll_x: 0.0,
        }
    }

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

}