// Nebula De Esser v2.2.0 — DSP Engine (SYNTAX CORRECTED)
// FIX: Replaced Vec in LookaheadDelay with fixed array to prevent allocation crashes.

#![allow(dead_code,unused_variables,clippy::too_many_arguments,clippy::needless_pass_by_ref_mut,clippy::cast_precision_loss,clippy::cast_possible_truncation)]
use std::f64::consts::PI;

const MAX_DELAY_SAMPLES: usize = 8192;

#[inline(always)] pub fn ftz(x:f64)->f64{if(x.to_bits()&0x7FF0_0000_0000_0000)==0{0.0}else{x}}
#[inline(always)] pub fn db_to_lin(db:f64)->f64{10.0_f64.powf(db/20.0)}
#[inline(always)] pub fn lin_to_db(x:f64)->f64{if x<f64::EPSILON{-120.0}else{20.0*x.log10()}}

#[derive(Clone,Default)]
pub struct BiquadState{pub x1:f64,x2:f64,y1:f64,y2:f64}

#[derive(Clone,Copy)]
pub struct BiquadCoeffs{b0:f64,b1:f64,b2:f64,a1:f64,a2:f64}
impl BiquadCoeffs{
    #[inline(always)]
    pub fn highpass(f:f64,q:f64,sr:f64)->Self{
        let w=2.0*PI*f/sr;let c=w.cos();let s=w.sin();let a=s/(2.0*q);
        let b0=(1.0+c)/2.0;let b1=-(1.0+c);let b2=b0;let a0=1.0+a;
        Self{b0:b0/a0,b1:b1/a0,b2:b2/a0,a1:(-2.0*c)/a0,a2:(1.0-a)/a0}
    }
    #[inline(always)]
    pub fn lowpass(f:f64,q:f64,sr:f64)->Self{
        let w=2.0*PI*f/sr;let c=w.cos();let s=w.sin();let a=s/(2.0*q);
        let b0=(1.0-c)/2.0;let b1=1.0-c;let b2=b0;let a0=1.0+a;
        Self{b0:b0/a0,b1:b1/a0,b2:b2/a0,a1:(-2.0*c)/a0,a2:(1.0-a)/a0}
    }
    #[inline(always)]
    pub fn bandpass_peak(f:f64,q:f64,sr:f64)->Self{
        let w=2.0*PI*f/sr;let c=w.cos();let s=w.sin();let a=s/(2.0*q);let a0=1.0+a;
        Self{b0:(s/2.0)/a0,b1:0.0,b2:-(s/2.0)/a0,a1:(-2.0*c)/a0,a2:(1.0-a)/a0}
    }
    #[inline(always)]
    pub fn bell(f:f64,q:f64,gain_db:f64,sr:f64)->Self{
        let w=2.0*PI*f/sr;let c=w.cos();let s=w.sin();
        let a=10.0_f64.powf(gain_db/40.0);
        let alpha=s/(2.0*q);
        let a0=1.0+alpha/a;
        Self{
            b0:(1.0+alpha*a)/a0,
            b1:(-2.0*c)/a0,
            b2:(1.0-alpha*a)/a0,
            a1:(-2.0*c)/a0,
            a2:(1.0-alpha/a)/a0,
        }
    }
    #[inline(always)]
    pub fn process(&self,st:&mut BiquadState,x:f64)->f64{
        let y=self.b0*x+self.b1*st.x1+self.b2*st.x2-self.a1*st.y1-self.a2*st.y2;
        st.x2=ftz(st.x1);st.x1=ftz(x);st.y2=ftz(st.y1);st.y1=ftz(y);st.y1
    }
}

#[derive(Clone,Default)]
pub struct SplitState{
    pub lp1:BiquadState, pub lp2:BiquadState, pub lp3:BiquadState,
}

#[derive(Clone,Debug)]
pub struct EnvelopeFollower{pub attack_coeff:f64, pub release_coeff:f64, pub envelope:f64}
impl EnvelopeFollower{
    pub fn new(a:f64,r:f64,sr:f64)->Self{
        let mk=|ms:f64|if ms<0.0001{ 0.0 } else { (-1.0/(ms*0.001*sr)).exp() };
        Self{attack_coeff:mk(a), release_coeff:mk(r), envelope:0.0}
    }
    #[inline(always)]
    pub fn process(&mut self,x:f64)->f64{
        let a=x.abs();
        self.envelope=if a>self.envelope{
            self.attack_coeff*(self.envelope-a)+a
        }else{
            self.release_coeff*(self.envelope-a)+a
        };
        self.envelope=ftz(self.envelope);self.envelope
    }
    pub fn reset(&mut self){self.envelope=0.0;}
}

pub struct LookaheadDelay{
    buffer: [f64; MAX_DELAY_SAMPLES],
    write_pos: usize,
    delay_samples: usize,
}
impl LookaheadDelay{
    pub fn new(_max_ms:f64,_sr:f64)->Self{
        Self{ buffer: [0.0; MAX_DELAY_SAMPLES], write_pos: 0, delay_samples: 0 }
    }
    pub fn set_delay(&mut self,ms:f64,sr:f64){
        let target_samples = (ms * 0.001 * sr).round() as usize;
        self.delay_samples = target_samples.min(MAX_DELAY_SAMPLES - 1);
    }
    #[inline(always)]
    pub fn process(&mut self,x:f64)->f64{
        self.buffer[self.write_pos] = x;
        let read_pos = if self.write_pos >= self.delay_samples {
            self.write_pos - self.delay_samples
        } else {
            MAX_DELAY_SAMPLES - self.delay_samples + self.write_pos
        };
        self.write_pos = (self.write_pos + 1) % MAX_DELAY_SAMPLES;
        self.buffer[read_pos]
    }
    pub fn reset(&mut self){ self.buffer.fill(0.0); self.write_pos = 0; }
}

#[inline(always)]
pub fn compute_gain_reduction(det:f64,thr:f64,mx:f64,knee:f64)->f64{
    let o=det-thr;
    if o < -knee*0.5 { 0.0 }
    else if o > knee*0.5 {
        let t=(o-knee*0.5)/mx; 
        t.min(1.0)
    } else {
        let t=(o+knee*0.5)/knee; 
        t*t*0.5
    }
}

#[derive(Clone)]
pub struct GainSmoother{stage:[f64;4],coeff:f64}
impl GainSmoother{
    pub fn new(a:f64,r:f64,sr:f64)->Self{
        let mk=|ms:f64|if ms<0.0001{ 0.0 } else { (-1.0/(ms*0.001*sr)).exp() };
        Self{stage:[0.0;4], coeff:mk(a.max(0.3))}
    }
    #[inline(always)]
    pub fn process(&mut self,t:f64)->f64{
        for s in &mut self.stage{
            let c=if t<f64::EPSILON{self.coeff}else{((-t).ln()/0.001).exp()};
            *s=*s+(t-*s)*c;
        }
        self.stage[3]
    }
    pub fn reset(&mut self){self.stage.fill(0.0);}
}

pub struct MakeupSmoother{coeff:f64,val:f64}
impl MakeupSmoother{
    pub fn new(sr:f64)->Self{Self{coeff:(-1.0/(200.0*0.001*sr)).exp(),val:0.0}}
    #[inline(always)]
    pub fn process(&mut self,gr_db:f64)->f64{
        let t=(-gr_db).max(0.0)*0.5;
        self.val=self.coeff*(self.val-t)+t;self.val=ftz(self.val);self.val
    }
    pub fn reset(&mut self){self.val=0.0;}
}

pub struct ChannelDsp{
    pub hp1:BiquadState,pub hp2:BiquadState,pub hp3:BiquadState,
    pub lp1:BiquadState,pub lp2:BiquadState,pub lp3:BiquadState,
    pub peak:BiquadState,
    pub split:SplitState,
    pub bell1:BiquadState,pub bell2:BiquadState,
    pub detect_env:EnvelopeFollower,
    pub full_env:EnvelopeFollower,
    pub gain_smoother:GainSmoother,
    pub makeup:MakeupSmoother,
    pub lookahead_audio:LookaheadDelay,
    pub lookahead_sidechain:LookaheadDelay,
}
impl ChannelDsp{
    pub fn new(sr:f64)->Self{
        Self{
            hp1:Default::default(),hp2:Default::default(),hp3:Default::default(),
            lp1:Default::default(),lp2:Default::default(),lp3:Default::default(),
            peak:Default::default(),
            split:SplitState::default(),
            bell1:Default::default(),bell2:Default::default(),
            detect_env:EnvelopeFollower::new(0.5,80.0,sr),
            full_env:EnvelopeFollower::new(0.5,80.0,sr),
            gain_smoother:GainSmoother::new(0.3,100.0,sr),
            makeup:MakeupSmoother::new(sr),
            lookahead_audio:LookaheadDelay::new(20.0,sr),
            lookahead_sidechain:LookaheadDelay::new(20.0,sr),
        }
    }
    pub fn reset(&mut self){
        for s in[&mut self.hp1,&mut self.hp2,&mut self.hp3,
                 &mut self.lp1,&mut self.lp2,&mut self.lp3,
                 &mut self.peak,&mut self.bell1,&mut self.bell2]{ *s=Default::default(); }
        self.split=SplitState::default();
        self.detect_env.reset();self.full_env.reset();
        self.gain_smoother.reset();self.makeup.reset();
        self.lookahead_audio.reset();self.lookahead_sidechain.reset();
    }
}

pub struct DeEsserDsp{
    pub channels:[ChannelDsp;2],
    pub sample_rate:f64,
    pub hp_c:[BiquadCoeffs;3], pub lp_c:[BiquadCoeffs;3], pub pk_c:BiquadCoeffs,
    pub split_lp_c:[BiquadCoeffs;3], pub bell_c:[BiquadCoeffs;2],
    pub center_freq:f64, pub cut_q:f64, pub cut_depth_db:f64,
}

impl DeEsserDsp{
    const BW6Q:[f64;3]=[0.5176,0.7071,1.9319];
    fn make_hp(f:f64,sr:f64)->[BiquadCoeffs;3]{[
         BiquadCoeffs::highpass(f,Self::BW6Q[0],sr),
         BiquadCoeffs::highpass(f,Self::BW6Q[1],sr),
         BiquadCoeffs::highpass(f,Self::BW6Q[2],sr)]}
    fn make_lp(f:f64,sr:f64)->[BiquadCoeffs;3]{[
         BiquadCoeffs::lowpass(f,Self::BW6Q[0],sr),
         BiquadCoeffs::lowpass(f,Self::BW6Q[1],sr),
         BiquadCoeffs::lowpass(f,Self::BW6Q[2],sr)]}

    pub fn new(sr:f64)->Self{
        Self{
            channels:[ChannelDsp::new(sr),ChannelDsp::new(sr)], sample_rate:sr,
            hp_c:Self::make_hp(6000.0,sr), lp_c:Self::make_lp(12000.0,sr),
            pk_c:BiquadCoeffs::bandpass_peak(8000.0,1.4,sr),
            split_lp_c:Self::make_lp(6000.0,sr),
            bell_c:[BiquadCoeffs::bell(8000.0,1.4,-12.0,sr);2],
            center_freq:8000.0, cut_q:1.4, cut_depth_db:-12.0,
        }
    }
    pub fn reset(&mut self){for c in &mut self.channels{c.reset();}}
    pub fn update_filters(&mut self,min_f:f64,max_f:f64,_use_peak:bool,cut_width:f64,cut_depth:f64,max_red:f64){
        let sr=self.sample_rate;
        let mn=min_f.clamp(20.0,sr*0.49); let mx=max_f.clamp(mn+10.0,sr*0.49);
        let ctr=(mn*mx).sqrt();
        let det_q=(ctr/(mx-mn).max(1.0)).clamp(0.5,6.0);
        let q_cut=(0.5+cut_width.clamp(0.0,1.0)*5.5).clamp(0.5,6.0);
        let depth_db=-(cut_depth.clamp(0.0,1.0)*max_red.abs());
        self.hp_c=Self::make_hp(mn,sr); self.lp_c=Self::make_lp(mx,sr);
        self.pk_c=BiquadCoeffs::bandpass_peak(ctr,det_q,sr);
        self.split_lp_c=Self::make_lp(ctr,sr);
        self.center_freq=ctr; self.cut_q=q_cut; self.cut_depth_db=depth_db;
        self.bell_c=[BiquadCoeffs::bell(ctr,q_cut,depth_db*0.6,sr),BiquadCoeffs::bell(ctr,q_cut*1.4,depth_db*0.4,sr)];
    }
    pub fn update_lookahead(&mut self,ms:f64){for c in &mut self.channels{c.lookahead_audio.set_delay(ms,self.sample_rate);c.lookahead_sidechain.set_delay(ms,self.sample_rate);}}
    pub fn update_envelope(&mut self,a:f64,r:f64){let sr=self.sample_rate;for c in &mut self.channels{c.detect_env=EnvelopeFollower::new(a,r,sr);c.full_env=EnvelopeFollower::new(a,r,sr);c.gain_smoother=GainSmoother::new(a.max(0.3),r*1.5,sr);}}
    #[inline(always)] fn detect_filter(&mut self,x:f64,ch:usize,use_peak:bool)->f64{
        let c=&mut self.channels[ch];
        if use_peak{self.pk_c.process(&mut c.peak,x)}else{
            let h=self.hp_c[0].process(&mut c.hp1,x);let h=self.hp_c[1].process(&mut c.hp2,h);let h=self.hp_c[2].process(&mut c.hp3,h);
            let l=self.lp_c[0].process(&mut c.lp1,h);let l=self.lp_c[1].process(&mut c.lp2,l);self.lp_c[2].process(&mut c.lp3,l)
        }
    }
    #[inline(always)] fn split_complement(&mut self,x:f64,ch:usize)->(f64,f64){
        let sp=&mut self.channels[ch].split;
        let l1=self.split_lp_c[0].process(&mut sp.lp1,x);let l2=self.split_lp_c[1].process(&mut sp.lp2,l1);
        let lo=self.split_lp_c[2].process(&mut sp.lp3,l2);(x-lo,lo)
    }
    #[inline(always)] fn apply_bell_cut(&mut self,x:f64,gain_lin:f64,ch:usize)->f64{
        let cut_amount=1.0-gain_lin.clamp(0.0,1.0);if cut_amount<f64::EPSILON{return x;}
        let c=&mut self.channels[ch];
        let b1=self.bell_c[0].process(&mut c.bell1,x);let b2=self.bell_c[1].process(&mut c.bell2,b1);
        x*gain_lin+b2*cut_amount
    }
    #[inline(always)] fn channel_gain(&mut self,ed:f64,ef:f64,thr:f64,mx:f64,rel:bool,knee:f64,ch:usize)->(f64,f64){
        let dd=lin_to_db(ed);let fd=lin_to_db(ef);
        let(di,ti)=if rel{(dd-fd,thr-20.0)}else{(dd,thr)};
        let gr=compute_gain_reduction(di,ti,mx,knee);
        let t=db_to_lin(gr);(self.channels[ch].gain_smoother.process(t),dd)
    }
    #[inline(always)]
    pub fn process_sample(&mut self,left_in:f64,right_in:f64,ext_l:Option<f64>,ext_r:Option<f64>,thr:f64,max_red:f64,relative:bool,use_peak:bool,use_wide:bool,stereo_link:f64,mid_side:bool,lookahead_en:bool,trigger_hear:bool,filter_solo:bool,auto_makeup:bool)->(f64,f64,f64,f64){
        let(l,r)=if mid_side{((left_in+right_in)*std::f64::consts::FRAC_1_SQRT_2,(left_in-right_in)*std::f64::consts::FRAC_1_SQRT_2)}else{(left_in,right_in)};
        let sc_l=ext_l.unwrap_or(l);let sc_r=ext_r.unwrap_or(r);
        let det_l=self.detect_filter(sc_l,0,use_peak);let det_r=self.detect_filter(sc_r,1,use_peak);
        let(al,ar)=if lookahead_en{(self.channels[0].lookahead_audio.process(l),self.channels[1].lookahead_audio.process(r))}else{(l,r)};
        let el=self.channels[0].detect_env.process(det_l);let er=self.channels[1].detect_env.process(det_r);
        let fl=self.channels[0].full_env.process(l.abs());let fr=self.channels[1].full_env.process(r.abs());
        let lnk=stereo_link.clamp(0.0,1.0);let ae=(el+er)*0.5;let af=(fl+fr)*0.5;
        let ell=el*(1.0-lnk)+ae*lnk;let erl=er*(1.0-lnk)+ae*lnk;let fll=fl*(1.0-lnk)+af*lnk;let frl=fr*(1.0-lnk)+af*lnk;
        let knee=4.0;
        let(gl,ddl)=self.channel_gain(ell,fll,thr,max_red,relative,knee,0);let(gr,ddr)=self.channel_gain(erl,frl,thr,max_red,relative,knee,1);
        let avg_gr_db=lin_to_db((gl+gr)*0.5);
        let(ol,or_)=if trigger_hear{(det_l,det_r)}else if filter_solo{(det_l*gl,det_r*gr)}else if use_wide{(self.apply_bell_cut(al,gl,0),self.apply_bell_cut(ar,gr,1))}else{let(hi_l,lo_l)=self.split_complement(al,0);let(hi_r,lo_r)=self.split_complement(ar,1);(lo_l+hi_l*gl,lo_r+hi_r*gr)};
        let(ol,or_)=if auto_makeup{let mul=db_to_lin(self.channels[0].makeup.process(avg_gr_db));let mur=db_to_lin(self.channels[1].makeup.process(avg_gr_db));(ol*mul,or_*mur)}else{(ol,or_)};
        let(fl_,fr_)=if mid_side{((ol+or_)*std::f64::consts::FRAC_1_SQRT_2,(ol-or_)*std::f64::consts::FRAC_1_SQRT_2)}else{(ol,or_)};
        (fl_,fr_,(ddl+ddr)*0.5,avg_gr_db)
    }
    pub fn process_block(&mut self,left:&[f64],right:&[f64],thr:f64,max_red:f64,relative:bool,use_peak:bool,use_wide:bool,stereo_link:f64,mid_side:bool,lookahead_en:bool,trigger_hear:bool,filter_solo:bool,auto_makeup:bool)->(Vec<f64>,Vec<f64>,Vec<f64>,Vec<f64>){
        let n=left.len();let mut ol=vec![0.0;n];let mut or_=vec![0.0;n];let mut det=vec![0.0;n];let mut red=vec![0.0;n];
        for i in 0..n{let(l,r,d,rv)=self.process_sample(left[i],right[i],None,None,thr,max_red,relative,use_peak,use_wide,stereo_link,mid_side,lookahead_en,trigger_hear,filter_solo,auto_makeup);ol[i]=l;or_[i]=r;det[i]=d;red[i]=rv;}
        (ol,or_,det,red)
    }
}
