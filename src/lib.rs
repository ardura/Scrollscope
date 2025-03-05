use atomic_float::{AtomicF32};
use itertools::{izip};
use nih_plug::{prelude::*};
use nih_plug_egui::EguiState;
use rustfft::{num_complex::Complex, FftPlanner};
use std::{env, sync::{atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicUsize, Ordering}, Arc}};
use std::sync::Mutex;

mod slim_checkbox;
mod scrollscope_gui;

/**************************************************
 * Scrollscope v1.4.1 by Ardura
 * "A simple scrolling Oscilloscope has become complex now"
 *
 * Build with: cargo xtask bundle scrollscope --profile release
 * Debug with: cargo xtask bundle scrollscope --profile profiling
 * 
 * If you don't want/need the standalone version you can save time by only compiling the VST + CLAP with "--lib"
 * cargo xtask bundle scrollscope --profile release --lib
 * ************************************************/

const MAX_BUFFER_SIZE: usize = 48000;
const NUM_CHANNELS: usize = 7; // Main + 5 aux + beat lines

struct LockFreeCircularBuffer {
    internal_length: AtomicUsize,
    buffers: Vec<Box<[AtomicF32]>>, // Heap-allocated storage
    write_indices: [AtomicUsize; NUM_CHANNELS],
}

impl LockFreeCircularBuffer {
    fn new(size: usize) -> Self {
        /*
        Self {
            internal_length: AtomicUsize::new(size),
            buffers: std::array::from_fn(|_| std::array::from_fn(|_| AtomicF32::new(0.0))),
            write_indices: std::array::from_fn(|_| AtomicUsize::new(0)),
        }
        */
        Self {
            internal_length: AtomicUsize::new(size),
            buffers: (0..NUM_CHANNELS)
                .map(|_| {
                    (0..MAX_BUFFER_SIZE)
                        .map(|_| AtomicF32::new(0.0))
                        .collect::<Vec<_>>()
                        .into_boxed_slice()
                })
                .collect(), // Collect into a Vec<Box<[AtomicF32]>>
            write_indices: std::array::from_fn(|_| AtomicUsize::new(0)),
        }
    }
    
    fn update_sample(&self, channel: usize, index: usize, sample: f32) {
        self.buffers[channel][index].store(sample, Ordering::Relaxed);
    }

    fn update_internal_length(&self, new_length: usize) {
        self.internal_length.store(new_length, Ordering::Relaxed);
    }

    fn push_sample(&self, channel: usize, sample: f32) {
        if channel >= NUM_CHANNELS { return; }

        let write_idx = self.write_indices[channel].fetch_add(1, Ordering::Relaxed) % self.internal_length.load(Ordering::Relaxed);
        self.buffers[channel][write_idx].store(sample, Ordering::Relaxed);
    }

    fn get_sample(&self, channel: usize, index: usize) -> Option<f32> {
        return Option::Some(self.buffers[channel][index].load(Ordering::Relaxed))
    }

    fn get_samples(&self, channel: usize) -> Vec<f32> {
        if channel >= NUM_CHANNELS { return Vec::new(); }

        let write_idx = self.write_indices[channel].load(Ordering::Relaxed);
        
        let local_int_len = self.internal_length.load(Ordering::Relaxed);
        let mut samples = Vec::with_capacity(local_int_len);
        for i in 0..local_int_len {
            let idx = (write_idx + i) % local_int_len;
            samples.push(self.buffers[channel][idx].load(Ordering::Relaxed));
        }
        samples
    }

    fn get_complex_samples_with_length(&self, channel: usize, length: usize) -> Vec<Complex<f32>> {
        if channel >= NUM_CHANNELS { return Vec::new(); }
        let local_int_len = self.internal_length.load(Ordering::Relaxed);

        let write_idx = self.write_indices[channel].load(Ordering::Relaxed);
        
        let mut complex_samples = Vec::with_capacity(length.min(local_int_len));
        for i in 0..length.min(local_int_len) {
            let idx = (write_idx + i) % local_int_len;
            let sample = self.buffers[channel][idx].load(Ordering::Relaxed);
            
            complex_samples.push(Complex::new(flush_denormal_bits(sample), 0.0));
        }
        complex_samples
    }
}

#[derive(Enum, Clone, PartialEq)]
pub enum BeatSync {
    Beat,
    Bar,
}

pub struct Scrollscope {
    params: Arc<ScrollscopeParams>,

    // Counter for scaling sample skipping
    skip_counter: Arc<AtomicI32>,
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
    sample_buffer: Arc<LockFreeCircularBuffer>,
    // Stereo field uses this second set
    sample_buffer_2: Arc<LockFreeCircularBuffer>,

    // Syncing for beats
    sync_var: Arc<AtomicBool>,
    alt_sync: Arc<AtomicBool>,
    in_place_index: Arc<AtomicI32>,
    beat_threshold: Arc<AtomicI32>,
    add_beat_line: Arc<AtomicBool>,

    // FFT/Analyzer
    fft: Arc<Mutex<FftPlanner<f32>>>,
    show_analyzer: Arc<AtomicBool>,
    en_filled_lines: Arc<AtomicBool>,

    en_filled_osc: Arc<AtomicBool>,

    // Stereo view
    stereo_view: Arc<AtomicBool>,

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
            skip_counter: Arc::new(AtomicI32::new(0)),
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
            
            sample_buffer: Arc::new(LockFreeCircularBuffer::new(130)),
            sample_buffer_2: Arc::new(LockFreeCircularBuffer::new(130)),

            sync_var: Arc::new(AtomicBool::new(false)),
            alt_sync: Arc::new(AtomicBool::new(false)),
            add_beat_line: Arc::new(AtomicBool::new(false)),
            in_place_index: Arc::new(AtomicI32::new(0)),
            beat_threshold: Arc::new(AtomicI32::new(0)),
            fft: Arc::new(Mutex::new(FftPlanner::new())),
            show_analyzer: Arc::new(AtomicBool::new(false)),
            en_filled_lines: Arc::new(AtomicBool::new(false)),
            en_filled_osc: Arc::new(AtomicBool::new(false)),
            stereo_view: Arc::new(AtomicBool::new(false)),
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
        // Inputs,Outputs,sidechain,No Idea but needed
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

    fn editor(&mut self, async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        scrollscope_gui::make_gui(self, async_executor)
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
            self.skip_counter.store(0, Ordering::SeqCst);

            // Get iterators outside the loop
            // These are immutable to not break borrows and the .to_iter() things that return borrows
            let raw_buffer = buffer.as_slice_immutable();
            let aux_0 = aux.inputs[0].as_slice_immutable();
            let aux_1 = aux.inputs[1].as_slice_immutable();
            let aux_2 = aux.inputs[2].as_slice_immutable();
            let aux_3 = aux.inputs[3].as_slice_immutable();
            let aux_4 = aux.inputs[4].as_slice_immutable();

            if !self.show_analyzer.load(Ordering::SeqCst) {
                let channels = [0,1];
                //                                                             [          CHANNEL         ]
                // Iterate over all inputs at the same time. they are in form [[[left, right],[left,right]],...]
                for (b0, ax0, ax1, ax2, ax3, ax4, channel) in
                    izip!(raw_buffer, aux_0, aux_1, aux_2, aux_3, aux_4, channels)
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
                            const EPSILON: f64 = 0.0001;
                            // Works in FL Studio but not other daws, hence the previous couple of lines
                            match self.params.sync_timing.value() {
                                BeatSync::Bar => {
                                    //temp_current_beat % 4.0 == 0.0
                                    if (temp_current_beat % 4.0) < EPSILON {
                                        // Reset our index to the sample vecdeques
                                        //self.in_place_index = Arc::new(Mutex::new(0));
                                        self.in_place_index.store(0, Ordering::SeqCst);
                                        self.skip_counter.store(0, Ordering::SeqCst);
                                    }
                                }
                                BeatSync::Beat => {
                                    //temp_current_beat % 1.0 == 0.0
                                    if (temp_current_beat % 1.0) < EPSILON {
                                        // Reset our index to the sample vecdeques
                                        //self.in_place_index = Arc::new(Mutex::new(0));
                                        self.in_place_index.store(0, Ordering::SeqCst);
                                        self.skip_counter.store(0, Ordering::SeqCst);
                                    }
                                }
                            }
                        }
                    }

                    // Reset the right side skipping getting out of control in stereo mode
                    //if channel == 1 {
                    //    self.skip_counter.store(0, Ordering::SeqCst);
                    //}

                    // Scrollscope is running as a single channel through here
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
                        ax4.iter(),
                    ) {
                        // Only grab X(skip_counter) samples to "optimize"
                        if self.skip_counter.load(Ordering::SeqCst) % self.params.h_scale.value() == 0 {
                            let current_gain = self.params.free_gain.smoothed.next();
                            // Apply gain to main signal
                            let visual_main_sample: f32 = sample * current_gain;
                            // Apply gain to sidechains if it isn't doubled up/cloned (FL Studio does this)
                            let visual_aux_sample_1 = if aux_sample_1 != sample {
                                aux_sample_1 * current_gain
                            } else {
                                0.0
                            };
                            let visual_aux_sample_2 = if aux_sample_2 != sample {
                                aux_sample_2 * current_gain
                            } else {
                                0.0
                            };
                            let visual_aux_sample_3 = if aux_sample_3 != sample {
                                aux_sample_3 * current_gain
                            } else {
                                0.0
                            };
                            let visual_aux_sample_4 = if aux_sample_4 != sample {
                                aux_sample_4 * current_gain
                            } else {
                                0.0
                            };
                            let visual_aux_sample_5 = if aux_sample_5 != sample {
                                aux_sample_5 * current_gain
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
                            
                            
                            /*
                            let mut guard;
                            let mut aux_guard;
                            let mut aux_guard_2;
                            let mut aux_guard_3;
                            let mut aux_guard_4;
                            let mut aux_guard_5;
                            
                            if channel == 0 {
                                // Update our main samples vector for oscilloscope drawing
                                guard = self.samples.lock().unwrap();
                                // Update our sidechain samples vector for oscilloscope drawing
                                aux_guard = self.aux_samples_1.lock().unwrap();
                                aux_guard_2 = self.aux_samples_2.lock().unwrap();
                                aux_guard_3 = self.aux_samples_3.lock().unwrap();
                                aux_guard_4 = self.aux_samples_4.lock().unwrap();
                                aux_guard_5 = self.aux_samples_5.lock().unwrap();
                            } else {
                                // Update our main samples vector for oscilloscope drawing
                                guard = self.samples_2.lock().unwrap();
                                // Update our sidechain samples vector for oscilloscope drawing
                                aux_guard = self.aux_samples_1_2.lock().unwrap();
                                aux_guard_2 = self.aux_samples_2_2.lock().unwrap();
                                aux_guard_3 = self.aux_samples_3_2.lock().unwrap();
                                aux_guard_4 = self.aux_samples_4_2.lock().unwrap();
                                aux_guard_5 = self.aux_samples_5_2.lock().unwrap();
                            }
                            
                            let mut sbl_guard = self.scrolling_beat_lines.lock().unwrap();
                            */
                            // If beat sync is on, we need to process changes in place
                            if self.sync_var.load(Ordering::SeqCst) {
                                // Access the in place index
                                let ipi_index: usize = self.in_place_index.load(Ordering::SeqCst) as usize;
                                // If we add a beat line, also clean all VecDeques past this index to line them up
                                if self.add_beat_line.load(Ordering::SeqCst) {
                                    if self.stereo_view.load(Ordering::SeqCst) {
                                        self.sample_buffer.push_sample(6, 2.1);
                                        self.sample_buffer.push_sample(6, -2.1);
                                    } else {
                                        self.sample_buffer.push_sample(6, 1.0);
                                        self.sample_buffer.push_sample(6, -1.0);
                                    }
                                    self.add_beat_line.store(false, Ordering::SeqCst);
                                    if self.alt_sync.load(Ordering::SeqCst) && self.params.sync_timing.value() == BeatSync::Beat {
                                        // Fix random crash where disable and enable sync attempts drain on unknown index
                                        /*
                                        if guard.get(ipi_index).is_some() {
                                            // This removes extra stuff on the right (jitter)
                                            guard.drain(ipi_index..);
                                            aux_guard.drain(ipi_index..);
                                            aux_guard_2.drain(ipi_index..);
                                            aux_guard_3.drain(ipi_index..);
                                            aux_guard_4.drain(ipi_index..);
                                            aux_guard_5.drain(ipi_index..);
                                        }
                                        */
                                    }
                                } else {
                                    self.sample_buffer.push_sample(6,0.0);
                                }

                                // Check if our indexes exists
                                if self.sample_buffer.get_sample(0, ipi_index).is_some() {
                                    // Modify our index since it exists (this compensates for scale/sample changes)
                                    self.sample_buffer.update_sample(0, ipi_index, visual_main_sample);
                                }
                                if self.sample_buffer.get_sample(1, ipi_index).is_some() {
                                    // Modify our index since it exists (this compensates for scale/sample changes)
                                    self.sample_buffer.update_sample(1, ipi_index, visual_aux_sample_1);
                                }
                                if self.sample_buffer.get_sample(2, ipi_index).is_some() {
                                    // Modify our index since it exists (this compensates for scale/sample changes)
                                    self.sample_buffer.update_sample(2, ipi_index, visual_aux_sample_2);
                                }
                                if self.sample_buffer.get_sample(3, ipi_index).is_some() {
                                    // Modify our index since it exists (this compensates for scale/sample changes)
                                    self.sample_buffer.update_sample(3, ipi_index, visual_aux_sample_3);
                                }
                                if self.sample_buffer.get_sample(4, ipi_index).is_some() {
                                    // Modify our index since it exists (this compensates for scale/sample changes)
                                    self.sample_buffer.update_sample(4, ipi_index, visual_aux_sample_4);
                                }
                                if self.sample_buffer.get_sample(5, ipi_index).is_some() {
                                    // Modify our index since it exists (this compensates for scale/sample changes)
                                    self.sample_buffer.update_sample(5, ipi_index, visual_aux_sample_5);
                                }

                                // Increment our in_place_index now that we have substituted
                                self.in_place_index.fetch_add(1, Ordering::SeqCst);
                            }
                            // Beat sync is off: allow "scroll"
                            else {
                                if channel == 0 {
                                    if self.add_beat_line.load(Ordering::SeqCst) {
                                        if self.stereo_view.load(Ordering::SeqCst) {
                                            self.sample_buffer.push_sample(6, 2.1);
                                            self.sample_buffer.push_sample(6, -2.1);
                                        } else {
                                            self.sample_buffer.push_sample(6, 1.0);
                                            self.sample_buffer.push_sample(6, -1.0);
                                        }
                                        
                                        self.add_beat_line.store(false, Ordering::SeqCst);
                                    } else {
                                        self.sample_buffer.push_sample(6, 0.0);
                                    }
                                }

                                self.sample_buffer.push_sample(0, visual_main_sample);
                                self.sample_buffer.push_sample(1, visual_aux_sample_1);
                                self.sample_buffer.push_sample(2, visual_aux_sample_2);
                                self.sample_buffer.push_sample(3, visual_aux_sample_3);
                                self.sample_buffer.push_sample(4, visual_aux_sample_4);
                                self.sample_buffer.push_sample(5, visual_aux_sample_5);
                            }
                            // ms = samples/samplerate so ms*samplerate = samples
                            // Limit the size of the vecdeques to X elements
                            //let scroll: usize = (sample_rate as usize / 1000_usize)
                            //    * self.params.scrollspeed.value() as usize;
                        }
                        
                        //if channel == 0 {
                            self.skip_counter.fetch_add(1, Ordering::SeqCst);
                        //}
                    }
                }
            } else {
                for (b0, ax0, ax1, ax2, ax3, ax4) in
                    izip!(raw_buffer, aux_0, aux_1, aux_2, aux_3, aux_4)
                {
                    if self.skip_counter.load(Ordering::SeqCst) % self.params.h_scale.value() == 0 {
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

                            // Update all the samples
                            self.sample_buffer.push_sample(0, visual_main_sample);
                            self.sample_buffer.push_sample(1, visual_aux_sample_1);
                            self.sample_buffer.push_sample(2, visual_aux_sample_2);
                            self.sample_buffer.push_sample(3, visual_aux_sample_3);
                            self.sample_buffer.push_sample(4, visual_aux_sample_4);
                            self.sample_buffer.push_sample(5, visual_aux_sample_5);

                            //let scroll: usize = (sample_rate as usize / 1000.0 as usize)
                            //        * self.params.scrollspeed.value() as usize;
                        }
                    }
                    self.skip_counter.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
        ProcessStatus::Normal
    }
    
    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    
    const HARD_REALTIME_ONLY: bool = false;
    
    fn task_executor(&mut self) -> TaskExecutor<Self> {
        // In the default implementation we can simply ignore the value
        Box::new(|_| ())
    }
    
    fn filter_state(_state: &mut PluginState) {}
    
    fn reset(&mut self) {}
    
    fn deactivate(&mut self) {}
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

fn flush_denormal_bits(val: f32) -> f32 {
    let bits = val.to_bits();
    let abs_bits = bits & 0x7fffffff; // Clear sign bit
    if abs_bits < 0x00800000 { // Check if denormal (exponent bits are all 0)
        0.0
    } else {
        val
    }
}