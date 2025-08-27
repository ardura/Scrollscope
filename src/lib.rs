use atomic_float::AtomicF32;
use itertools::izip;
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use rustfft::{num_complex::Complex, FftPlanner};
use std::{
    env,
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicUsize, Ordering},
        Arc, Mutex, RwLock,
    },
};

mod slim_checkbox;
mod scrollscope_gui;

/**************************************************
 * Scrollscope v1.4.3 by Ardura
 * "A simple scrolling Oscilloscope has become complex now"
 * ************************************************/

// Calculate buffer size dynamically based on sample rate and max display time
fn calculate_buffer_size(sample_rate: f32, max_display_ms: f32) -> usize {
    ((sample_rate * max_display_ms) / 1000.0).ceil() as usize
}

const NUM_CHANNELS: usize = 8; // Main + 5 aux + beat lines + sum
const MAX_DISPLAY_MS: f32 = 1000.0; // Maximum display time in milliseconds

// Channel data container
struct ChannelBuffer {
    data: Vec<f32>,
    write_index: usize,
}

impl ChannelBuffer {
    fn new(size: usize) -> Self {
        Self {
            data: vec![0.0; size],
            write_index: 0,
        }
    }

    fn resize(&mut self, new_size: usize) {
        if new_size > self.data.len() {
            self.data.resize(new_size, 0.0);
        }
        // Don't shrink - avoid reallocation
    }

    fn push_sample(&mut self, sample: f32, buffer_len: usize) {
        self.data[self.write_index % buffer_len] = sample;
        self.write_index = (self.write_index + 1) % buffer_len;
    }

    fn update_sample(&mut self, index: usize, sample: f32) {
        if index < self.data.len() {
            self.data[index] = sample;
        }
    }

    fn zero_out(&mut self) {
        for x in 0..self.data.len() {
            self.data[x] = 0.0;
        }
        self.write_index = 0;
    }

    fn get_sample(&self, index: usize) -> Option<f32> {
        if index < self.data.len() {
            Some(self.data[index])
        } else {
            None
        }
    }

    fn get_samples(&self, buffer_len: usize) -> Vec<f32> {
        let mut samples = Vec::with_capacity(buffer_len);
        let start_idx = self.write_index;
        
        for i in 0..buffer_len {
            let idx = (start_idx + i) % buffer_len;
            samples.push(self.data[idx]);
        }
        
        samples
    }

    fn get_complex_samples(&self, length: usize, buffer_len: usize) -> Vec<Complex<f32>> {
        let mut complex_samples = Vec::with_capacity(length.min(buffer_len));
        let start_idx = self.write_index;
        
        for i in 0..length.min(buffer_len) {
            let idx = (start_idx + i) % buffer_len;
            let sample = self.data[idx];
            complex_samples.push(Complex::new(flush_denormal_bits(sample), 0.0));
        }
        
        complex_samples
    }
}

// More efficient buffer implementation
struct OptimizedBuffer {
    internal_length: AtomicUsize,
    // Use RwLock for better read concurrency
    buffers: RwLock<Vec<ChannelBuffer>>,
}

impl OptimizedBuffer {
    fn new(size: usize) -> Self {
        let mut channels = Vec::with_capacity(NUM_CHANNELS);
        for _ in 0..NUM_CHANNELS {
            channels.push(ChannelBuffer::new(size));
        }
        
        Self {
            internal_length: AtomicUsize::new(size),
            buffers: RwLock::new(channels),
        }
    }
    
    fn update_internal_length(&self, new_length: usize) {
        self.internal_length.store(new_length, Ordering::Release);
        let mut buffers = self.buffers.write().unwrap();
        for channel in buffers.iter_mut() {
            channel.resize(new_length);
        }
    }

    // Batch process multiple samples with a single lock acquisition
    fn push_samples(&self, channel_data: &[(usize, f32)]) {
        let buffer_len = self.internal_length.load(Ordering::Acquire);
        let mut buffers = self.buffers.write().unwrap();
        
        for &(channel, sample) in channel_data {
            if channel < NUM_CHANNELS {
                buffers[channel].push_sample(sample, buffer_len);
            }
        }
    }

    fn update_sample(&self, channel: usize, index: usize, sample: f32) {
        if channel >= NUM_CHANNELS {
            return;
        }

        let buffers = self.buffers.write().unwrap();
        if let Some(buffer) = buffers.get(channel) {
            if index < buffer.data.len() {
                // Use const cast to avoid double locking
                unsafe {
                    let buffer_ptr = buffer as *const ChannelBuffer as *mut ChannelBuffer;
                    (*buffer_ptr).update_sample(index, sample);
                }
            }
        }
    }

    fn get_sample(&self, channel: usize, index: usize) -> Option<f32> {
        if channel >= NUM_CHANNELS {
            return None;
        }

        let buffers = self.buffers.read().unwrap();
        buffers[channel].get_sample(index)
    }

    fn get_samples(&self, channel: usize) -> Vec<f32> {
        if channel >= NUM_CHANNELS {
            return Vec::new();
        }

        let buffer_len = self.internal_length.load(Ordering::Acquire);
        let buffers = self.buffers.read().unwrap();
        buffers[channel].get_samples(buffer_len)
    }

    fn get_complex_samples_with_length(&self, channel: usize, length: usize) -> Vec<Complex<f32>> {
        if channel >= NUM_CHANNELS {
            return Vec::new();
        }

        let buffer_len = self.internal_length.load(Ordering::Acquire);
        let buffers = self.buffers.read().unwrap();
        buffers[channel].get_complex_samples(length, buffer_len)
    }
}

#[derive(Enum, Clone, PartialEq)]
pub enum BeatSync {
    Beat,
    Bar,
}

pub struct Scrollscope {
    params: Arc<ScrollscopeParams>,

    // Counter for scaling sample skipping - use a local counter to reduce atomic ops
    skip_counter: [Arc<AtomicI32>; 2],
    focused_line_toggle: Arc<AtomicU8>,
    is_clipping: Arc<AtomicF32>,
    direction: Arc<AtomicBool>,
    
    // Channel visibility flags
    channel_enabled: [Arc<AtomicBool>; 7], // main + 5 aux + sum

    // Gui flags
    enable_sum: Arc<AtomicBool>,
    enable_guidelines: Arc<AtomicBool>,
    enable_bar_mode: Arc<AtomicBool>,
    
    // Data holding values - optimized buffer implementation
    sample_buffer: Arc<OptimizedBuffer>,
    sample_buffer_2: Arc<OptimizedBuffer>,

    // Syncing for beats - cached to reduce atomic ops
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
    en_left_channel: Arc<AtomicBool>,
    en_right_channel: Arc<AtomicBool>,

    sample_rate: Arc<AtomicF32>,
    prev_skip: Arc<AtomicI32>,
    
    // Cache frequently accessed parameters
    gain_cache: Arc<AtomicF32>,
    h_scale_cache: Arc<AtomicI32>,
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
        // Initialize with reasonable defaults based on 44.1kHz
        let initial_buffer_size = calculate_buffer_size(44100.0, MAX_DISPLAY_MS);
        
        Self {
            params: Arc::new(ScrollscopeParams::default()),
            skip_counter: [Arc::new(AtomicI32::new(0)), Arc::new(AtomicI32::new(0))],
            focused_line_toggle: Arc::new(AtomicU8::new(0)),
            direction: Arc::new(AtomicBool::new(false)),
            is_clipping: Arc::new(AtomicF32::new(0.0)),
            
            // Replace individual AtomicBools with an array for simpler access
            channel_enabled: [
                Arc::new(AtomicBool::new(true)),  // main
                Arc::new(AtomicBool::new(false)), // aux_1
                Arc::new(AtomicBool::new(false)), // aux_2
                Arc::new(AtomicBool::new(false)), // aux_3
                Arc::new(AtomicBool::new(false)), // aux_4
                Arc::new(AtomicBool::new(false)), // aux_5
                Arc::new(AtomicBool::new(true)),  // sum
            ],
            en_left_channel: Arc::new(AtomicBool::new(true)),
            en_right_channel: Arc::new(AtomicBool::new(true)),

            enable_sum: Arc::new(AtomicBool::new(true)),
            enable_guidelines: Arc::new(AtomicBool::new(true)),
            enable_bar_mode: Arc::new(AtomicBool::new(false)),
            
            sample_buffer: Arc::new(OptimizedBuffer::new(initial_buffer_size)),
            sample_buffer_2: Arc::new(OptimizedBuffer::new(initial_buffer_size)),

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
            
            // Cache parameters for faster access
            gain_cache: Arc::new(AtomicF32::new(1.0)),
            h_scale_cache: Arc::new(AtomicI32::new(24)),
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
            scrollspeed: FloatParam::new("Length", 100.0, FloatRange::Skewed { min: 1.0, max: 1000.0, factor: 0.33 })
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

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[new_nonzero_u32(2); 5],
            aux_output_ports: &[],
            names: PortNames {
                layout: Option::None,
                main_input: Some("Main Input"),
                aux_inputs: &[
                    "Aux 1",
                    "Aux 2",
                    "Aux 3",
                    "Aux 4",
                    "Aux 5",
                ],
                main_output: Option::None,
                aux_outputs: &[]
            }
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

    fn editor(&self, async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        scrollscope_gui::make_gui(self, async_executor)
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        // Update sample rate and buffer size on initialization
        let sample_rate = buffer_config.sample_rate;
        self.sample_rate.store(sample_rate, Ordering::Release);
        
        // Calculate appropriate buffer size based on sample rate and max display time
        let buffer_size = calculate_buffer_size(sample_rate, MAX_DISPLAY_MS);
        self.sample_buffer.update_internal_length(buffer_size);
        self.sample_buffer_2.update_internal_length(buffer_size);
        
        true
    }

    fn process(
        &mut self,
        buffer: &mut nih_plug::prelude::Buffer<'_>,
        aux: &mut nih_plug::prelude::AuxiliaryBuffers<'_>,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Only process if the GUI is open
        if !self.params.editor_state.is_open() {
            return ProcessStatus::Normal;
        }

        // Update cached parameters to reduce atomic loads inside the loop
        let current_gain = self.params.free_gain.smoothed.next();
        self.gain_cache.store(current_gain, Ordering::Relaxed);
        
        let h_scale = self.params.h_scale.value();
        self.h_scale_cache.store(h_scale, Ordering::Relaxed);
        
        // Update sample rate if it changed
        let sample_rate = context.transport().sample_rate;
        if sample_rate != self.sample_rate.load(Ordering::Relaxed) {
            self.sample_rate.store(sample_rate, Ordering::Release);
            
            // Recalculate buffer size based on new sample rate
            let buffer_size = calculate_buffer_size(sample_rate, self.params.scrollspeed.value());
            self.sample_buffer.update_internal_length(buffer_size);
            self.sample_buffer_2.update_internal_length(buffer_size);
        }
        
        // Reset skip counter before processing
        let mut local_skip_counter = [0,0];
        self.skip_counter[0].store(0, Ordering::Relaxed);
        self.skip_counter[1].store(0, Ordering::Relaxed);

        // Determine whether to process in analyzer mode or oscilloscope mode
        if !self.show_analyzer.load(Ordering::Relaxed) {
            // Process in oscilloscope mode
            self.process_oscilloscope(buffer, aux, context, &mut local_skip_counter);
        } else {
            // Process in analyzer mode
            self.process_analyzer(buffer, aux, &mut local_skip_counter);
        }
        
        // Update the skip counter
        self.skip_counter[0].store(local_skip_counter[0], Ordering::Relaxed);
        self.skip_counter[1].store(local_skip_counter[1], Ordering::Relaxed);
        
        ProcessStatus::Normal
    }
    
    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const HARD_REALTIME_ONLY: bool = false;
    
    fn task_executor(&self) -> TaskExecutor<Self> {
        Box::new(|_| ())
    }
    
    fn filter_state(_state: &mut PluginState) {}
    fn reset(&mut self) {}
    fn deactivate(&mut self) {}
}

// Separate processing implementations to reduce complexity
impl Scrollscope {
    fn process_oscilloscope(
        &self,
        buffer: &mut nih_plug::buffer::Buffer<'_>,
        aux: &mut nih_plug::audio_setup::AuxiliaryBuffers<'_>,
        context: &mut impl ProcessContext<Self>,
        skip_counter: &mut [i32; 2],
    ) {
        // Get buffer slices for efficient processing
        let raw_buffer = buffer.as_slice_immutable();
        let aux_0 = aux.inputs[0].as_slice_immutable();
        let aux_1 = aux.inputs[1].as_slice_immutable();
        let aux_2 = aux.inputs[2].as_slice_immutable();
        let aux_3 = aux.inputs[3].as_slice_immutable();
        let aux_4 = aux.inputs[4].as_slice_immutable();
        
        // Cache parameters to avoid atomic loads in the loop
        let h_scale = self.h_scale_cache.load(Ordering::Relaxed) as i32;
        let current_gain = self.gain_cache.load(Ordering::Relaxed);
        let sync_active = self.sync_var.load(Ordering::Relaxed);
        let alt_sync_active = self.alt_sync.load(Ordering::Relaxed);
        let stereo_mode = self.stereo_view.load(Ordering::Relaxed);
        
        // Process beat detection once per buffer instead of per sample
        let (is_on_beat, is_on_bar) = self.detect_beat(context);
        let mut add_beat_line = false;
        
        // Update in-place index if needed
        let mut in_place_idx = self.in_place_index.load(Ordering::Relaxed);
        if sync_active {
            if alt_sync_active {
                if context.transport().playing {
                    match self.params.sync_timing.value() {
                        BeatSync::Bar => {
                            if is_on_bar && self.beat_threshold.load(Ordering::Relaxed) == 0 {
                                in_place_idx = 0;
                                self.beat_threshold.fetch_add(1, Ordering::Relaxed);
                            } else if !is_on_bar && self.beat_threshold.load(Ordering::Relaxed) > 0 {
                                self.beat_threshold.store(0, Ordering::Relaxed);
                            }
                        },
                        BeatSync::Beat => {
                            if is_on_beat && self.beat_threshold.load(Ordering::Relaxed) == 0 {
                                in_place_idx = 0;
                                self.beat_threshold.fetch_add(1, Ordering::Relaxed);
                            } else if !is_on_beat && self.beat_threshold.load(Ordering::Relaxed) > 0 {
                                self.beat_threshold.store(0, Ordering::Relaxed);
                            }
                        }
                    }
                } else {
                    in_place_idx = 0;
                }
            } else {
                // Normal sync mode
                let current_beat = context.transport().pos_beats().unwrap();
                let temp_current_beat = (current_beat * 1000.0).round() / 1000.0;
                
                const EPSILON: f64 = 0.001;
                match self.params.sync_timing.value() {
                    BeatSync::Bar => {
                        if (temp_current_beat % 4.0) < EPSILON {
                            in_place_idx = 0;
                            skip_counter[0] = 0;
                            skip_counter[1] = 0;
                            let mut buffers = self.sample_buffer.buffers.write();
                            for buff in buffers.iter_mut() {
                                buff[0].zero_out();
                                buff[1].zero_out();
                                buff[2].zero_out();
                                buff[3].zero_out();
                                buff[4].zero_out();
                                buff[5].zero_out();
                                buff[6].zero_out();
                                buff[7].zero_out();
                            }
                            let mut buffers = self.sample_buffer_2.buffers.write();
                            for buff in buffers.iter_mut() {
                                buff[0].zero_out();
                                buff[1].zero_out();
                                buff[2].zero_out();
                                buff[3].zero_out();
                                buff[4].zero_out();
                                buff[5].zero_out();
                                buff[6].zero_out();
                                buff[7].zero_out();
                            }
                        }
                    }
                    BeatSync::Beat => {
                        if (temp_current_beat % 1.0) < EPSILON {
                            in_place_idx = 0;
                            skip_counter[0] = 0;
                            skip_counter[1] = 0;
                            let mut buffers = self.sample_buffer.buffers.write();
                            for buff in buffers.iter_mut() {
                                buff[0].zero_out();
                                buff[1].zero_out();
                                buff[2].zero_out();
                                buff[3].zero_out();
                                buff[4].zero_out();
                                buff[5].zero_out();
                                buff[6].zero_out();
                                buff[7].zero_out();
                            }
                            let mut buffers = self.sample_buffer_2.buffers.write();
                            for buff in buffers.iter_mut() {
                                buff[0].zero_out();
                                buff[1].zero_out();
                                buff[2].zero_out();
                                buff[3].zero_out();
                                buff[4].zero_out();
                                buff[5].zero_out();
                                buff[6].zero_out();
                                buff[7].zero_out();
                            }
                        }
                    }
                }
            }
        }
        
        // Check if we need to add beat lines
        if context.transport().playing {
            if alt_sync_active {
                add_beat_line = is_on_beat;
                self.in_place_index.store(0, Ordering::SeqCst);
            } else if ((context.transport().pos_beats().unwrap() * 1000.0).round() / 1000.0) % 1.0 == 0.0 {
                add_beat_line = true;
                if sync_active {
                    self.in_place_index.store(0, Ordering::SeqCst);
                }
            }
        }
        
        // Process all channels in stereo mode
        let channels = [0, 1];
        for (b0, ax0, ax1, ax2, ax3, ax4, channel) in 
            izip!(raw_buffer, aux_0, aux_1, aux_2, aux_3, aux_4, channels) {
            
            // Setup batch processing
            let mut l_batch = Vec::with_capacity(100); // Pre-allocate to avoid reallocations
            let mut r_batch = Vec::with_capacity(100); // Pre-allocate to avoid reallocations
            
            // Process all samples in this channel
            for (sample, aux_sample_1, aux_sample_2, aux_sample_3, aux_sample_4, aux_sample_5) in 
                izip!(b0.iter(), ax0.iter(), ax1.iter(), ax2.iter(), ax3.iter(), ax4.iter()) {
                
                // Only process samples according to h_scale parameter
                if (channel == 0 && skip_counter[0] % h_scale == 0) || (channel == 1 && skip_counter[1] % h_scale == 0) {
                    // Apply gain to samples
                    let visual_main_sample = sample * current_gain;
                    
                    // Only apply aux processing if the aux isn't the same as the main signal
                    let visual_aux_sample_1 = if *aux_sample_1 != *sample { *aux_sample_1 * current_gain } else { 0.0 };
                    let visual_aux_sample_2 = if *aux_sample_2 != *sample { *aux_sample_2 * current_gain } else { 0.0 };
                    let visual_aux_sample_3 = if *aux_sample_3 != *sample { *aux_sample_3 * current_gain } else { 0.0 };
                    let visual_aux_sample_4 = if *aux_sample_4 != *sample { *aux_sample_4 * current_gain } else { 0.0 };
                    let visual_aux_sample_5 = if *aux_sample_5 != *sample { *aux_sample_5 * current_gain } else { 0.0 };

                    let mut sum_sample = 0.0;
                    if self.channel_enabled[6].load(Ordering::Relaxed) {
                        if self.channel_enabled[0].load(Ordering::Relaxed) {
                            sum_sample += visual_main_sample;
                        }
                        if self.channel_enabled[1].load(Ordering::Relaxed) {
                            sum_sample += visual_aux_sample_1;
                        }
                        if self.channel_enabled[2].load(Ordering::Relaxed) {
                            sum_sample += visual_aux_sample_2;
                        }
                        if self.channel_enabled[3].load(Ordering::Relaxed) {
                            sum_sample += visual_aux_sample_3;
                        }
                        if self.channel_enabled[4].load(Ordering::Relaxed) {
                            sum_sample += visual_aux_sample_4;
                        }
                        if self.channel_enabled[5].load(Ordering::Relaxed) {
                            sum_sample += visual_aux_sample_5;
                        }
                    }
                    
                    // Check for clipping
                    if visual_main_sample.abs() > 1.0 || 
                       visual_aux_sample_1.abs() > 1.0 || 
                       visual_aux_sample_2.abs() > 1.0 || 
                       visual_aux_sample_3.abs() > 1.0 || 
                       visual_aux_sample_4.abs() > 1.0 || 
                       visual_aux_sample_5.abs() > 1.0 {
                        self.is_clipping.store(120.0, Ordering::Relaxed);
                    }
                    
                    // Process based on sync mode
                    if sync_active {
                        // In-place update mode
                        let ipi_index = in_place_idx as usize;
                        
                        if self.sample_buffer.get_sample(0, ipi_index).is_some() {
                            self.sample_buffer.update_sample(0, ipi_index, visual_main_sample);
                            self.sample_buffer.update_sample(1, ipi_index, visual_aux_sample_1);
                            self.sample_buffer.update_sample(2, ipi_index, visual_aux_sample_2);
                            self.sample_buffer.update_sample(3, ipi_index, visual_aux_sample_3);
                            self.sample_buffer.update_sample(4, ipi_index, visual_aux_sample_4);
                            self.sample_buffer.update_sample(5, ipi_index, visual_aux_sample_5);
                            //6 is beat lines
                            if add_beat_line {
                                if stereo_mode {
                                    self.sample_buffer.update_sample(6, ipi_index, 2.1);
                                    if ipi_index > 0 {
                                        self.sample_buffer.update_sample(6, ipi_index - 1, -2.1);
                                    } else {
                                        self.sample_buffer.update_sample(6, ipi_index + 1, -2.1);
                                    }
                                } else {
                                    self.sample_buffer.update_sample(6, ipi_index, 1.0);
                                    if ipi_index > 0 {
                                        self.sample_buffer.update_sample(6, ipi_index - 1, -1.0);
                                    } else {
                                        self.sample_buffer.update_sample(6, ipi_index + 1, -1.0);
                                    }
                                }
                            }
                            self.sample_buffer.update_sample(7, ipi_index, sum_sample);
                        } 
                        
                        if self.sample_buffer_2.get_sample(0, ipi_index).is_some() {
                            self.sample_buffer_2.update_sample(0, ipi_index, visual_main_sample);
                            self.sample_buffer_2.update_sample(1, ipi_index, visual_aux_sample_1);
                            self.sample_buffer_2.update_sample(2, ipi_index, visual_aux_sample_2);
                            self.sample_buffer_2.update_sample(3, ipi_index, visual_aux_sample_3);
                            self.sample_buffer_2.update_sample(4, ipi_index, visual_aux_sample_4);
                            self.sample_buffer_2.update_sample(5, ipi_index, visual_aux_sample_5);
                            //6 is beat lines
                            self.sample_buffer_2.update_sample(7, ipi_index, sum_sample);
                        }
                        
                        if channel == 1 {
                            // Increment in-place index
                            in_place_idx += 1;
                        }
                    } else {
                        if channel == 0 {
                            // Add beat line if needed (only on first channel)
                            if add_beat_line {
                                if stereo_mode {
                                    l_batch.push((6, 2.1));
                                    l_batch.push((6, -2.1));
                                } else {
                                    l_batch.push((6, 1.0));
                                    l_batch.push((6, -1.0));
                                }

                                add_beat_line = false; // Reset flag after adding
                                self.add_beat_line.store(false, Ordering::Relaxed);
                            } else {
                                l_batch.push((6, 0.0)); // Normal point for beat channel
                            }
                            // Normal scrolling mode - add samples to batch
                            l_batch.push((0, visual_main_sample));
                            l_batch.push((1, visual_aux_sample_1));
                            l_batch.push((2, visual_aux_sample_2));
                            l_batch.push((3, visual_aux_sample_3));
                            l_batch.push((4, visual_aux_sample_4));
                            l_batch.push((5, visual_aux_sample_5));
                            l_batch.push((7, sum_sample));
                        } else {
                            // Normal scrolling mode - add samples to batch
                            r_batch.push((0, visual_main_sample));
                            r_batch.push((1, visual_aux_sample_1));
                            r_batch.push((2, visual_aux_sample_2));
                            r_batch.push((3, visual_aux_sample_3));
                            r_batch.push((4, visual_aux_sample_4));
                            r_batch.push((5, visual_aux_sample_5));
                            r_batch.push((7, sum_sample));
                        }
                    }
                    
                    // Process batch if it's getting large
                    if l_batch.len() >= 50 {
                        if channel == 0 {
                            self.sample_buffer.push_samples(&l_batch);
                        }
                        l_batch.clear();
                    }
                    if r_batch.len() >= 50 {
                        if channel == 1 {
                            self.sample_buffer_2.push_samples(&r_batch);
                        }
                        r_batch.clear();
                    }
                }

                skip_counter[channel] += 1;
            }
            
            // Process any remaining samples in batch
            if !l_batch.is_empty() {
                if channel == 0 {
                    self.sample_buffer.push_samples(&l_batch);
                }
            }
            if !r_batch.is_empty() {
                if channel == 1 {
                    self.sample_buffer_2.push_samples(&r_batch);
                }
            }
        }
        // Store updated in-place index
        self.in_place_index.store(in_place_idx, Ordering::Relaxed);
    }
    
    fn process_analyzer(
        &self,
        buffer: &mut nih_plug::prelude::Buffer<'_>,
        aux: &mut nih_plug::prelude::AuxiliaryBuffers<'_>,
        skip_counter: &mut [i32; 2],
    ) {
        // Get buffer slices for efficient processing
        let raw_buffer = buffer.as_slice_immutable();
        let aux_0 = aux.inputs[0].as_slice_immutable();
        let aux_1 = aux.inputs[1].as_slice_immutable();
        let aux_2 = aux.inputs[2].as_slice_immutable();
        let aux_3 = aux.inputs[3].as_slice_immutable();
        let aux_4 = aux.inputs[4].as_slice_immutable();
        
        // Cache parameters to avoid atomic loads in the loop
        let h_scale = self.h_scale_cache.load(Ordering::Relaxed) as i32;
        let current_gain = self.gain_cache.load(Ordering::Relaxed);
        
        // Process all channels
        let channels = [0, 1];
        for (b0, ax0, ax1, ax2, ax3, ax4, channel) in 
            izip!(raw_buffer, aux_0, aux_1, aux_2, aux_3, aux_4, channels) {
            
            // Setup batch processing
            let mut batch = Vec::with_capacity(100); // Pre-allocate to avoid reallocations
            
            // Process all samples in this channel
            for (sample, aux_sample_1, aux_sample_2, aux_sample_3, aux_sample_4, aux_sample_5) in 
                izip!(b0.iter(), ax0.iter(), ax1.iter(), ax2.iter(), ax3.iter(), ax4.iter()) {
                
                // Only process samples according to h_scale parameter
                if (channel == 0 && skip_counter[0] % h_scale == 0) || (channel == 1 && skip_counter[1] % h_scale == 0) {
                    // Apply gain to samples
                    let visual_main_sample = sample * current_gain;
                    
                    // Only apply aux processing if the aux isn't the same as the main signal
                    let visual_aux_sample_1 = if *aux_sample_1 != *sample { *aux_sample_1 * current_gain } else { 0.0 };
                    let visual_aux_sample_2 = if *aux_sample_2 != *sample { *aux_sample_2 * current_gain } else { 0.0 };
                    let visual_aux_sample_3 = if *aux_sample_3 != *sample { *aux_sample_3 * current_gain } else { 0.0 };
                    let visual_aux_sample_4 = if *aux_sample_4 != *sample { *aux_sample_4 * current_gain } else { 0.0 };
                    let visual_aux_sample_5 = if *aux_sample_5 != *sample { *aux_sample_5 * current_gain } else { 0.0 };
                    
                    // Check for clipping
                    if visual_main_sample.abs() > 1.0 || 
                       visual_aux_sample_1.abs() > 1.0 || 
                       visual_aux_sample_2.abs() > 1.0 || 
                       visual_aux_sample_3.abs() > 1.0 || 
                       visual_aux_sample_4.abs() > 1.0 || 
                       visual_aux_sample_5.abs() > 1.0 {
                        self.is_clipping.store(120.0, Ordering::Relaxed);
                    }
                    
                    // Add samples to batch
                    batch.push((0, visual_main_sample));
                    batch.push((1, visual_aux_sample_1));
                    batch.push((2, visual_aux_sample_2));
                    batch.push((3, visual_aux_sample_3));
                    batch.push((4, visual_aux_sample_4));
                    batch.push((5, visual_aux_sample_5));
                    
                    // Process batch if it's getting large
                    if batch.len() >= 50 {
                        if channel == 0 {
                            self.sample_buffer.push_samples(&batch);
                        } else {
                            self.sample_buffer_2.push_samples(&batch);
                        }
                        batch.clear();
                    }
                }

                skip_counter[channel] += 1;
            }
            
            // Process any remaining samples in batch
            if !batch.is_empty() {
                if channel == 0 {
                    self.sample_buffer.push_samples(&batch);
                } else {
                    self.sample_buffer_2.push_samples(&batch);
                }
            }
        }
    }
    
    // Helper method to detect beats from transport
    fn detect_beat(&self, context: &mut impl ProcessContext<Self>) -> (bool, bool) {
        let mut is_on_beat = false;
        let mut is_on_bar = false;
        
        if context.transport().playing {
            if let Some(beats) = context.transport().pos_beats() {
                // Check if we're on a beat (integer value)
                let beat_fractional = beats.fract();
                // Use epsilon comparison for floating point
                if beat_fractional < 0.01 || beat_fractional > 0.99 {
                    is_on_beat = true;
                    
                    // Check if we're on a bar (every 4 beats typically)
                    let beat_in_bar = beats % 4.0;
                    if beat_in_bar < 0.01 || beat_in_bar > 3.99 {
                        is_on_bar = true;
                    }
                }
            }
        }
        
        (is_on_beat, is_on_bar)
    }
}

// Helper function to eliminate denormals for better performance
#[inline]
fn flush_denormal_bits(value: f32) -> f32 {
    if value.abs() < 1.0e-20 {
        0.0
    } else {
        value
    }
}

impl ClapPlugin for Scrollscope {
    const CLAP_ID: &'static str = "com.ardura.scrollscope";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A scrolling oscilloscope");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Utility,
        ClapFeature::Analyzer,
    ];
}

impl Vst3Plugin for Scrollscope {
    const VST3_CLASS_ID: [u8; 16] = *b"ScrollscopeAAAAA";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Analyzer,
        Vst3SubCategory::Tools,
    ];
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