use ringbuf::{HeapRb, Rb, Producer, Consumer};
use std::sync::Arc;
use nih_plug::prelude::Buffer;

pub struct StableBlockAdapter {
    input_producer: Producer<f64, Arc<HeapRb<f64>>>,
    input_consumer: Consumer<f64, Arc<HeapRb<f64>>>,
    output_producer: Producer<f64, Arc<HeapRb<f64>>>,
    output_consumer: Consumer<f64, Arc<HeapRb<f64>>>,
    
    internal_block_size: usize,
    num_channels: usize,
    temp_interleaved: Vec<f64>,
}

impl StableBlockAdapter {
    pub fn new(block_size: usize, num_channels: usize) -> Self {
        // 8x buffer size provides plenty of "safety margin" for jittery DAWs
        let capacity = block_size * num_channels * 8;
        
        let rb_in = HeapRb::<f64>::new(capacity);
        let (prod_in, cons_in) = rb_in.split();
        
        let rb_out = HeapRb::<f64>::new(capacity);
        let (prod_out, cons_out) = rb_out.split();

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

    /// The "Shield" logic: Protects your DSP from erratic DAW block sizes
    pub fn process_shielded<F>(&mut self, buffer: &mut Buffer, mut dsp_callback: F)
    where
        F: FnMut(&mut [f64]),
    {
        let num_samples = buffer.samples();
        
        // 1. Capture DAW input (Interleave and Push)
        for i in 0..num_samples {
            for channel in 0..self.num_channels {
                let sample = buffer.as_slice()[channel][i];
                let _ = self.input_producer.push(sample as f64);
            }
        }

        // 2. Process in STABLE blocks
        let samples_per_internal_block = self.internal_block_size * self.num_channels;
        while self.input_consumer.len() >= samples_per_internal_block {
            // Fill our internal workspace
            for i in 0..samples_per_internal_block {
                self.temp_interleaved[i] = self.input_consumer.pop().unwrap_or(0.0);
            }

            // RUN THE FUTURISTIC DSP
            dsp_callback(&mut self.temp_interleaved);

            // Push processed samples back to output ring
            for i in 0..samples_per_internal_block {
                let _ = self.output_producer.push(self.temp_interleaved[i]);
            }
        }

        // 3. Output back to DAW
        for i in 0..num_samples {
            for channel in 0..self.num_channels {
                if let Some(out_sample) = self.output_consumer.pop() {
                    buffer.as_slice()[channel][i] = out_sample as f32;
                } else {
                    // Safety fallback: If output isn't ready, pass dry or silence
                    buffer.as_slice()[channel][i] = 0.0;
                }
            }
        }
    }
}
