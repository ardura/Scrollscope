// src/main.rs for standalone only

use nih_plug::prelude::*;
use scrollscope::Scrollscope;

fn main() {
    nih_export_standalone::<Scrollscope>();
}