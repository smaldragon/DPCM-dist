// author: smal

#[macro_use]
extern crate vst;
extern crate time;

use vst::buffer::AudioBuffer;
use vst::plugin::{Category, HostCallback, Info, Plugin, PluginParameters};
use vst::util::AtomicFloat;

use std::sync::Arc;

/// Simple Gain Effect.
/// Note that this does not use a proper scale for sound and shouldn't be used in
/// a production amplification effect!  This is purely for demonstration purposes,
/// as well as to keep things simple as this is meant to be a starting point for
/// any effect.
struct EffectData {
    queue_d: [i32;32],
    queue_n: [i32;32],
    queue_i: usize,
    delta_o: i32,
    delta_v: i32,
    zero_count: i32,
    sample_wait: f32,
}
impl Default for EffectData {
    fn default() -> EffectData {
        EffectData{
            queue_d: [0;32],     // the queue being output
            queue_n: [0;32],     // the queue being made
            queue_i: 0,          // the queue index
            delta_v: 0,                         // the current delta
            delta_o: 0,                         // the output delta
            zero_count: 0,                      // disables the dpcm if enough zero samples have passed
            sample_wait: 0.0,
        }
    }
}
struct GainEffect {
    // Store a handle to the plugin's parameter object.
    params: Arc<GainEffectParameters>,
    data: [EffectData;2],
    sample_rate: f32,
}

/// The plugin's parameter object contains the values of parameters that can be
/// adjusted from the host.  If we were creating an effect that didn't allow the
/// user to modify it at runtime or have any controls, we could omit this part.
///
/// The parameters object is shared between the processing and GUI threads.
/// For this reason, all mutable state in the object has to be represented
/// through thread-safe interior mutability. The easiest way to achieve this
/// is to store the parameters in atomic containers.
struct GainEffectParameters {
    amplitude: AtomicFloat,
    depth: AtomicFloat,
    reversebit: AtomicFloat,
    mix: AtomicFloat,
}
impl Default for GainEffectParameters {
    fn default() -> GainEffectParameters {
        GainEffectParameters {
            amplitude: AtomicFloat::new(0.25),
            depth: AtomicFloat::new(0.25),
            reversebit: AtomicFloat::new(0.0),
            mix: AtomicFloat::new(1.0),
        }
    }
}

// All plugins using `vst` also need to implement the `Plugin` trait.  Here, we
// define functions that give necessary info to our host.
impl Plugin for GainEffect {
    fn new(_host: HostCallback) -> Self {
        // Note that controls will always return a value from 0 - 1.
        // Setting a default to 0.5 means it's halfway up.
        GainEffect {
            params: Arc::new(GainEffectParameters::default()),
            data: [EffectData::default(),EffectData::default()],
            sample_rate: 44100.0,
        }
    }

    fn get_info(&self) -> Info {
        Info {
            name: "DPCM Distortion Reloaded".to_string(),
            vendor: "smal".to_string(),
            unique_id: 243723072,
            version: 1,
            inputs: 2,
            outputs: 2,
            // This `parameters` bit is important; without it, none of our
            // parameters will be shown!
            parameters: 4,
            category: Category::Effect,
            ..Default::default()
        }
    }
    
    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
    }

    // Here is where the bulk of our audio processing code goes.
    fn process(&mut self, buffer: &mut AudioBuffer<f32>) {
        // Read the amplitude from the parameter object
        let amplitude = self.params.amplitude.get() * 4.0;
        let reversebit = (self.params.reversebit.get()*32.0) as usize;
        let mix = self.params.mix.get();
        let depth = (self.params.depth.get()*255.0+1.0) as i32;
        // First, we destructure our audio buffer into an arbitrary number of
        // input and output buffers.  Usually, we'll be dealing with stereo (2 of each)
        // but that might change.
        let mut c :usize = 0;
        for (input_buffer, output_buffer) in buffer.zip() {
            // Next, we'll loop through each individual sample so we can apply the amplitude
            // value to it.
            for (input_sample, output_sample) in input_buffer.iter().zip(output_buffer) {
                self.data[c].sample_wait += self.sample_rate / 33144.0;
                let advance_delta = self.data[c].sample_wait >= 1.0;
                if advance_delta {
                    self.data[c].sample_wait -= 1.0;
                    let comp = (*input_sample * (depth as f32) * amplitude) as i32;
                    let _comp_back = (comp as f32)/(depth as f32);
                    if (comp >= self.data[c].delta_v && self.data[c].delta_v < depth) || self.data[c].delta_v <= -depth {
                        self.data[c].queue_n[self.data[c].queue_i] = 1;
                        self.data[c].delta_v += 1;
                    } else {
                        self.data[c].queue_n[self.data[c].queue_i] = -1;
                        self.data[c].delta_v -= 1;
                    }
                    /*else if comp < self.data[c].delta_v || comp != 0 || true {
                        self.data[c].queue_n[self.data[c].queue_i] = -1;
                        self.data[c].delta_v -= 1;
                    } else {
                        self.data[c].queue_n[self.data[c].queue_i] = 0;
                    }
                    */
                    self.data[c].delta_o += self.data[c].queue_d[self.data[c].queue_i];
                    
                    
                    // Clamp the deltas:
                    if self.data[c].delta_v > depth {
                        self.data[c].delta_v = depth;
                    } else if self.data[c].delta_v < -depth {
                        self.data[c].delta_v = -depth;
                    }
                    if self.data[c].delta_o > depth {
                        self.data[c].delta_o = depth;
                    } else if self.data[c].delta_o < -depth {
                        self.data[c].delta_o = -depth;
                    }
                    
                    
                    /*let comp_sample;
                    if comp != 0 {
                         comp_sample = (self.data[c].delta_o as f32)/16.0;
                    } else {
                        comp_sample = 0.0;
                    }*/
                    
                    if comp == 0 {
                        self.data[c].zero_count += 1;   
                    } else {
                        self.data[c].zero_count = 0;
                    }
                }
                let comp_sample;
                if self.data[c].zero_count < 16 {
                    comp_sample = (self.data[c].delta_o as f32)/(depth as f32);
                } else {
                    comp_sample = 0.0;
                }
                if advance_delta {
                    self.data[c].queue_i += 1;
                    if (reversebit != 0 && self.data[c].queue_i >= reversebit) || (reversebit == 0 && self.data[c].queue_i >= 8){
                    //if self.data[c].queue_i >= 8 {
                        self.data[c].queue_i = 0;
                        if reversebit == 0 {
                            self.data[c].queue_d = self.data[c].queue_n;
                        } else {
                        //self.data[c].queue_d[0] = self.data[c].queue_n[0];
                            for i in 0..reversebit {
                                self.data[c].queue_d[i] = self.data[c].queue_n[reversebit-i-1];
                            }
                        }
                    }
                }
                
                *output_sample = (((comp_sample * mix)) + (*input_sample * (1.0-mix) * amplitude))/amplitude;
            }
            c += 1;
        }
    }

    // Return the parameter object. This method can be omitted if the
    // plugin has no parameters.
    fn get_parameter_object(&mut self) -> Arc<dyn PluginParameters> {
        Arc::clone(&self.params) as Arc<dyn PluginParameters>
    }
}

impl PluginParameters for GainEffectParameters {
    // the `get_parameter` function reads the value of a parameter.
    fn get_parameter(&self, index: i32) -> f32 {
        match index {
            0 => self.amplitude.get(),
            1 => self.depth.get(),
            2 => self.reversebit.get(),
            3 => self.mix.get(),
            _ => 0.0,
        }
    }

    // the `set_parameter` function sets the value of a parameter.
    fn set_parameter(&self, index: i32, val: f32) {
        #[allow(clippy::single_match)]
        match index {
            0 => self.amplitude.set(val),
            1 => self.depth.set(val),
            2 => self.reversebit.set((( (val*32.0) as i32) as f32)/32.0),
            3 => self.mix.set(val),
            _ => (),
        }
    }

    // This is what will display underneath our control.  We can
    // format it into a string that makes the most since.
    fn get_parameter_text(&self, index: i32) -> String {
        match index {
            0 => format!("{:.2}", (self.amplitude.get()) * 4f32),
            1 => format!("{}", (self.depth.get()*255.0+1.0) as i32),
            2 => format!("{}", ((self.reversebit.get()*32.0) as i32)),
            3 => format!("{:.2}%", (self.mix.get()*100.0)),
            _ => "".to_string(),
        }
    }

    // This shows the control's name.
    fn get_parameter_name(&self, index: i32) -> String {
        match index {
            0 => "Amplitude",
            1 => "Depth",
            2 => "Reverse Byte",
            3 => "Mix",
            _ => "",
        }
        .to_string()
    }
}

// This part is important!  Without it, our plugin won't work.
plugin_main!(GainEffect);