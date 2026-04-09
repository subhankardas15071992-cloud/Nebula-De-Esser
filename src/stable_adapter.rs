use ringbuf::{HeapRb, Rb, Producer, Consumer};
use nih_plug::prelude::Buffer;

pub struct StableBlockAdapter {
    input_producer: Producer<f64, std::sync::Arc<HeapRb<f64>>>,
    input_consumer: Consumer<f64, std::sync::Arc<HeapRb<f64>>>,
    output_producer: Producer<f64, std::sync::Arc<HeapRb<f64>>>,
    output_consumer: Consumer<f64, std::sync::Arc<HeapRb<f64>>>,
    
    internal_block_size: usize,
    num_channels: usize,
    temp_interleaved: Vec<f64>,
}

impl StableBlockAdapter {
    pub fn new(block_size: usize, num_channels: usize) -> Self {
        // Capacity: block_size * channels * safety_multiplier
        // 8x is plenty for jittery DAWs
        let capacity = block_size * num_channels * 8;
        
        let rb_in = HeapRb::<f64>::new(capacity);
        let (prod_in, cons_in) = rb_in.split();
        
        let rb_out = HeapRb::<f64>::new(capacity);
        let (mut prod_out, cons_out) = rb_out.split();

        // IMPORTANT: Pre-fill the output buffer with silence.
        // This creates the actual "latency" that allows the ring buffer to work.
        // Without this, the DAW will try to pop samples that haven't been processed yet.
        for _ in 0..(block_size * num_channels) {
            let _ = prod_out.push(0.0);
        }

        Self {
            input_producer: prod_in,
            input_consumer: cons_in,
            output_producer: prod_out,
            output_consumer: cons_out,
            internal_block_size: block_size,
            num_channels,
            temp_interleaved: vec![0.0; block_size * num_channels],
        }
    }

    /// The "Shield" logic: Decouples DAW block size from DSP block size.
    pub fn process_shielded<F>(&mut self, buffer: &mut Buffer, mut dsp_callback: F)
    where
        F: FnMut(&mut [f64]),
    {
        let num_samples = buffer.samples();
        let num_channels = buffer.channels();

        // 1. Capture DAW input (Interleave and Push to Input Ring)
        for i in 0..num_samples {
            for channel in 0..num_channels {
                let sample = buffer.as_slice()[channel][i];
                let _ = self.input_producer.push(sample as f64);
            }
        }

        // 2. Process in STABLE blocks
        let samples_per_internal_block = self.internal_block_size * self.num_channels;
        
        // As long as we have enough samples for a full internal block, process them.
        while self.input_consumer.len() >= samples_per_internal_block {
            // Pop samples into our temporary workspace
            for i in 0..samples_per_internal_block {
                self.temp_interleaved[i] = self.input_consumer.pop().unwrap_or(0.0);
            }

            // Execute the 64-bit DSP
            dsp_callback(&mut self.temp_interleaved);

            // Push processed samples into the output ring
            for i in 0..samples_per_internal_block {
                let _ = self.output_producer.push(self.temp_interleaved[i]);
            }
        }

        // 3. Output back to DAW (De-interleave from Output Ring)
        for i in 0..num_samples {
            for channel in 0..num_channels {
                if let Some(out_sample) = self.output_consumer.pop() {
                    buffer.as_slice()[channel][i] = out_sample as f32;
                } else {
                    // Fallback to silence if the buffer is exhausted (should not happen if latency is set correctly)
                    buffer.as_slice()[channel][i] = 0.0;
                }
            }
        }
    }
}
