/**************************************************
 * Scrollscope Oscilloscpe by Ardura
 **************************************************/
 
 use atomic_float::AtomicF32;
  use nih_plug_vizia::vizia::prelude::*;
 use nih_plug_vizia::vizia::vg;
 use nih_plug_vizia::vizia::vg::{Color, Canvas};
 use std::sync::Arc;

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
    pub fn new<LensXScale, LensSamples, LensScrollX>(
        cx: &mut Context,
        x_scale: LensXScale, 
        scroll_x: LensScrollX,
        samples: LensSamples
    ) -> Handle<Self> 
        where
        LensXScale: Lens<Target = Arc<AtomicF32>>,
            LensSamples: Lens<Target = Arc<Vec<AtomicF32>>>, 
            LensScrollX: Lens<Target = Arc<AtomicF32>>,
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
        
        pub fn update_vals(&mut self, scroll_x: f32) {
            
            self.scroll_x = Arc::new(AtomicF32::new(scroll_x));
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
