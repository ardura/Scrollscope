// src/main.rs for standalone only

use nih_plug::prelude::*;
use scrollscope::Scrollscope as Scrollscope_Standalone;

fn main() {
    nih_export_standalone::<Scrollscope_Standalone>();
}