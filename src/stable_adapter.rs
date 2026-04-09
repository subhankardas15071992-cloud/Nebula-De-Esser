use ringbuf::{HeapRb, Rb, Producer, Consumer};
use nih_plug::prelude::*;
use std::sync::Arc;

pub struct StableBlockAdapter {
    // Main Audio Rings
    in_prod: Producer<f64, Arc<HeapRb<f64>>>,
    in_cons: Consumer<f64, Arc<HeapRb<f64>>>,
    out_prod: Producer<f64, Arc<HeapRb<f64>>>,
    out_cons: Consumer<f64, Arc<HeapRb<f64>>>,

    // Sidechain Rings
    sc_prod: Producer<f64, Arc<HeapRb<f64>>>,
    sc_cons: Consumer<f64, Arc<HeapRb<f64>>>,
    
    internal_block_size: usize,
    num_channels: usize,
    temp_in: Vec<f64>,
    temp_sc: Vec<f64>,
    temp_out: Vec<f64>,
}

impl StableBlockAdapter {
    pub fn new(block_size: usize, num_channels: usize) -> Self {
        let capacity = block_size * num_channels * 16; // Larger margin for stability
        
        let (in_prod, in_cons) = HeapRb::<f64>::new(capacity).split();
        let (mut out_prod, out_cons) = HeapRb::<f64>::new(capacity).split();
        let (sc_prod, sc_cons) = HeapRb::<f64>::new(capacity).split();

        // Prime the output buffer with silence to establish latency
        for _ in 0..(block_size * num_channels) {
            let _ = out_prod.push(0.0);
        }

        Self {
            in_prod, in_cons,
            out_prod, out_cons,
            sc_prod, sc_cons,
            internal_block_size: block_size,
            num_channels,
            temp_in: vec![0.0; block_size * num_channels],
            temp_sc: vec![0.0; block_size * num_channels],
            temp_out: vec![0.0; block_size * num_channels],
        }
    }

    pub fn process_shielded<F>(
        &mut self, 
        buffer: &mut Buffer, 
        aux: &mut AuxiliaryBuffers, 
        mut dsp_callback: F
    ) where
        F: FnMut(&[f64], &[f64], &mut [f64]),
    {
        let num_samples = buffer.samples();
        let num_channels = self.num_channels;

        // 1. Push DAW main input and Sidechain input into ring buffers
        let main_slice = buffer.as_slice();
        let have_sc = !aux.inputs.is_empty();
        
        for i in 0..num_samples {
            for ch in 0..num_channels {
                let s = main_slice[ch][i] as f64;
                let _ = self.in_prod.push(s);

                if have_sc {
                    let sc_s = aux.inputs[0].as_slice()[ch][i] as f64;
                    let _ = self.sc_prod.push(sc_s);
                } else {
                    let _ = self.sc_prod.push(0.0);
                }
            }
        }

        // 2. Process all available full blocks
        let samples_per_block = self.internal_block_size * num_channels;
        while self.in_cons.len() >= samples_per_block {
            for i in 0..samples_per_block {
                self.temp_in[i] = self.in_cons.pop().unwrap_or(0.0);
                self.temp_sc[i] = self.sc_cons.pop().unwrap_or(0.0);
            }

            // Execute the DSP closure
            dsp_callback(&self.temp_in, &self.temp_sc, &mut self.temp_out);

            for i in 0..samples_per_block {
                let _ = self.out_prod.push(self.temp_out[i]);
            }
        }

        // 3. Pop from output ring back to DAW
        for i in 0..num_samples {
            for ch in 0..num_channels {
                buffer.as_slice()[ch][i] = self.out_cons.pop().unwrap_or(0.0) as f32;
            }
        }
    }
}
