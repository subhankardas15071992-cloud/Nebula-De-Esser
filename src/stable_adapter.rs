use ringbuf::{HeapRb, traits::{Split, Producer, Consumer}};
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
    
    pub internal_block_size: usize,
    num_channels: usize,
    
    // Internal work buffers to avoid allocations in the process loop
    temp_in_l: Vec<f64>,
    temp_in_r: Vec<f64>,
    temp_sc_l: Vec<f64>,
    temp_sc_r: Vec<f64>,
    temp_out_l: Vec<f64>,
    temp_out_r: Vec<f64>,
}

impl StableBlockAdapter {
    pub fn new(block_size: usize, num_channels: usize) -> Self {
        // 16x capacity to handle extreme jitter in older DAWs
        let capacity = block_size * num_channels * 16;
        
        let (in_prod, in_cons) = HeapRb::<f64>::new(capacity).split();
        let (mut out_prod, out_cons) = HeapRb::<f64>::new(capacity).split();
        let (sc_prod, sc_cons) = HeapRb::<f64>::new(capacity).split();

        // Prime the output buffer with silence to account for latency
        for _ in 0..(block_size * num_channels) {
            let _ = out_prod.push(0.0);
        }

        Self {
            in_prod, in_cons,
            out_prod, out_cons,
            sc_prod, sc_cons,
            internal_block_size: block_size,
            num_channels,
            temp_in_l: vec![0.0; block_size],
            temp_in_r: vec![0.0; block_size],
            temp_sc_l: vec![0.0; block_size],
            temp_sc_r: vec![0.0; block_size],
            temp_out_l: vec![0.0; block_size],
            temp_out_r: vec![0.0; block_size],
        }
    }

    /// Shield the DSP logic from variable DAW block sizes.
    pub fn process_shielded<F>(
        &mut self, 
        buffer: &mut Buffer, 
        aux: &mut AuxiliaryBuffers, 
        mut dsp_callback: F
    ) where
        F: FnMut(&[f64], &[f64], &[f64], &[f64], &mut [f64], &mut [f64]),
    {
        let num_samples = buffer.samples();
        let main_slice = buffer.as_slice();
        let have_sc = !aux.inputs.is_empty();

        // 1. Capture main and sidechain input
        for i in 0..num_samples {
            // Channel 0 (Left)
            let _ = self.in_prod.push(main_slice[0][i] as f64);
            if have_sc {
                let sc_slice = aux.inputs[0].as_slice();
                let sc_val = if sc_slice.len() > 0 { sc_slice[0][i] as f64 } else { 0.0 };
                let _ = self.sc_prod.push(sc_val);
            } else {
                let _ = self.sc_prod.push(0.0);
            }

            // Channel 1 (Right)
            let r_val = if main_slice.len() > 1 { main_slice[1][i] as f64 } else { main_slice[0][i] as f64 };
            let _ = self.in_prod.push(r_val);
            if have_sc {
                let sc_slice = aux.inputs[0].as_slice();
                let sc_val = if sc_slice.len() > 1 { sc_slice[1][i] as f64 } else if sc_slice.len() > 0 { sc_slice[0][i] as f64 } else { 0.0 };
                let _ = self.sc_prod.push(sc_val);
            } else {
                let _ = self.sc_prod.push(0.0);
            }
        }

        // 2. Process fixed internal blocks
        while self.in_cons.len() >= (self.internal_block_size * 2) {
            // De-interleave from ring buffer into work buffers
            for i in 0..self.internal_block_size {
                self.temp_in_l[i] = self.in_cons.pop().unwrap_or(0.0);
                self.temp_in_r[i] = self.in_cons.pop().unwrap_or(0.0);
                self.temp_sc_l[i] = self.sc_cons.pop().unwrap_or(0.0);
                self.temp_sc_r[i] = self.sc_cons.pop().unwrap_or(0.0);
            }

            // Execute actual DSP
            dsp_callback(
                &self.temp_in_l, &self.temp_in_r, 
                &self.temp_sc_l, &self.temp_sc_r, 
                &mut self.temp_out_l, &mut self.temp_out_r
            );

            // Re-interleave back into output ring
            for i in 0..self.internal_block_size {
                let _ = self.out_prod.push(self.temp_out_l[i]);
                let _ = self.out_prod.push(self.temp_out_r[i]);
            }
        }

        // 3. Deliver to DAW
        for i in 0..num_samples {
            buffer.as_slice()[0][i] = self.out_cons.pop().unwrap_or(0.0) as f32;
            if buffer.channels() > 1 {
                buffer.as_slice()[1][i] = self.out_cons.pop().unwrap_or(0.0) as f32;
            } else {
                let _ = self.out_cons.pop(); // Drain the second channel sample
            }
        }
    }
}
