//! SHINECRAFT: an EffectCraft effect plug-in (WebAssembly ABI 2, see EffectCraft's docs/plugins.md).
//! Light rays stream out of the bright parts of a layer, away from a source point.
//!
//! Pipeline: emission (Base On, threshold, mask) at full resolution natively, on the 480 px working
//! grid in WebAssembly → radial blur toward the source (a running sum along rays in polar
//! coordinates, sampling that emission) onto the working grid (960 px natively) → shimmer, boost,
//! fractal noise, colorize → upsample and blend.
//!
//! Not available to WebAssembly plug-ins, so left out: Source Point Type / Name, Noise Type,
//! Noise Point Name, Parallax Z Depth (they need comp cameras and lights).
//!
//! Uses parameter `group`s, `ec_padding` (Extend Past Layer) and `ec_flags` where the host supports
//! them; older hosts ignore all three.

use std::cell::RefCell;
use std::f32::consts::TAU;

pub const MANIFEST: &str = r#"{
  "api": 1,
  "id": "io.github.spoofah.shinecraft",
  "name": "SHINECRAFT",
  "category": "Stylize",
  "version": "1.0.1",
  "author": "Spoofah",
  "description": "Volumetric light rays streaming from the bright parts of the layer, with shimmer, gradient colorize and fractal noise.",
  "params": [
    {"id": "threshold", "group": "Pre-Process", "name": "Threshold", "type": "slider", "default": 0, "min": 0, "max": 255, "decimals": 1},
    {"id": "use_mask", "group": "Pre-Process", "name": "Use Mask", "type": "checkbox", "default": false},
    {"id": "mask_radius", "group": "Pre-Process", "name": "Mask Radius", "type": "slider", "default": 100, "min": 0, "max": 4000, "sliderMax": 1000, "decimals": 1},
    {"id": "mask_feather", "group": "Pre-Process", "name": "Mask Feather", "type": "slider", "default": 50, "min": 0, "max": 4000, "sliderMax": 500, "decimals": 1},
    {"id": "source", "name": "Source Point", "type": "point", "default": [0.5, 0.5]},
    {"id": "ray_length", "name": "Ray Length", "type": "slider", "default": 4, "min": 0, "max": 1000, "sliderMax": 10, "decimals": 1},
    {"id": "shimmer_amount", "group": "Shimmer", "name": "Shimmer Amount", "type": "slider", "default": 0, "min": 0, "max": 10000, "sliderMax": 100, "decimals": 1},
    {"id": "shimmer_detail", "group": "Shimmer", "name": "Shimmer Detail", "type": "slider", "default": 10, "min": 0, "max": 10000, "sliderMax": 100, "decimals": 1},
    {"id": "shimmer_source", "group": "Shimmer", "name": "Source Point affects Shimmer", "type": "checkbox", "default": false},
    {"id": "shimmer_radius", "group": "Shimmer", "name": "Shimmer Radius", "type": "slider", "default": 80, "min": 1, "max": 10000, "sliderMax": 1000, "decimals": 1},
    {"id": "reduce_flicker", "group": "Shimmer", "name": "Reduce Flickering", "type": "checkbox", "default": false},
    {"id": "phase", "group": "Shimmer", "name": "Shimmer Phase", "type": "angle", "default": 0},
    {"id": "use_loop", "group": "Shimmer", "name": "Use Loop", "type": "checkbox", "default": false},
    {"id": "loop_revs", "group": "Shimmer", "name": "Revolutions in Loop", "type": "slider", "default": 1, "min": 1, "max": 100, "sliderMax": 10, "decimals": 0},
    {"id": "boost", "name": "Boost Light", "type": "slider", "default": 0, "min": 0, "max": 1000, "sliderMax": 10, "decimals": 1},
    {"id": "colorize", "group": "Colorize", "name": "Colorize...", "type": "popup", "options": ["None", "One Color", "3-Color Gradient", "5-Color Gradient"], "default": 2},
    {"id": "base_on", "group": "Colorize", "name": "Base On...", "type": "popup", "options": ["Lightness", "Luminance", "Alpha", "Alpha Edges", "Red", "Green", "Blue"], "default": 0},
    {"id": "highlights", "group": "Colorize", "name": "Highlights", "type": "color", "default": [1.0, 1.0, 1.0, 1.0]},
    {"id": "mid_high", "group": "Colorize", "name": "Mid High", "type": "color", "default": [1.0, 0.9, 0.55, 1.0]},
    {"id": "midtones", "group": "Colorize", "name": "Midtones", "type": "color", "default": [1.0, 0.65, 0.0, 1.0]},
    {"id": "mid_low", "group": "Colorize", "name": "Mid Low", "type": "color", "default": [0.98, 0.72, 0.55, 1.0]},
    {"id": "shadows", "group": "Colorize", "name": "Shadows", "type": "color", "default": [1.0, 0.0, 0.0, 1.0]},
    {"id": "edge_thickness", "group": "Colorize", "name": "Edge Thickness", "type": "slider", "default": 1, "min": 1, "max": 10, "decimals": 0},
    {"id": "fractal_enable", "group": "Fractal Noise", "name": "Fractal Noise", "type": "checkbox", "default": false},
    {"id": "fractal_center", "group": "Fractal Noise", "name": "Fractal Center", "type": "point", "default": [0.5, 0.5]},
    {"id": "x_speed", "group": "Fractal Noise", "name": "Fractal X Speed", "type": "slider", "default": 0, "min": -100, "max": 100, "decimals": 1},
    {"id": "y_speed", "group": "Fractal Noise", "name": "Fractal Y Speed", "type": "slider", "default": 0, "min": -100, "max": 100, "decimals": 1},
    {"id": "evo_speed", "group": "Fractal Noise", "name": "Evolution Speed", "type": "slider", "default": 5, "min": 0, "max": 50, "decimals": 1},
    {"id": "fractal_brightness", "group": "Fractal Noise", "name": "Fractal Brightness", "type": "slider", "default": 100, "min": 0, "max": 400, "decimals": 1},
    {"id": "fractal_contrast", "group": "Fractal Noise", "name": "Fractal Contrast", "type": "slider", "default": 100, "min": 0, "max": 400, "decimals": 1},
    {"id": "fractal_opacity", "group": "Fractal Noise", "name": "Fractal Opacity", "type": "slider", "default": 50, "min": 0, "max": 100, "decimals": 1},
    {"id": "fractal_size", "group": "Fractal Noise", "name": "Fractal Size", "type": "slider", "default": 25, "min": 1, "max": 100, "decimals": 1},
    {"id": "fractal_complexity", "group": "Fractal Noise", "name": "Fractal Complexity", "type": "slider", "default": 3, "min": 1, "max": 4, "decimals": 0},
    {"id": "fractal_rotation", "group": "Fractal Noise", "name": "Fractal Rotation", "type": "angle", "default": 0},
    {"id": "use_noise_mask", "group": "Fractal Noise/Noise Mask", "name": "Use Noise Mask", "type": "checkbox", "default": false},
    {"id": "fractal_mask_radius", "group": "Fractal Noise/Noise Mask", "name": "Fractal Mask Radius", "type": "slider", "default": 100, "min": 0, "max": 4000, "sliderMax": 1000, "decimals": 1},
    {"id": "fractal_mask_feather", "group": "Fractal Noise/Noise Mask", "name": "Fractal Mask Feather", "type": "slider", "default": 50, "min": 0, "max": 4000, "sliderMax": 500, "decimals": 1},
    {"id": "fractal_blend", "group": "Fractal Noise", "name": "Fractal Blend Mode", "type": "popup", "options": ["Add", "Multiply", "Screen", "Lighten"], "default": 0},
    {"id": "fractal_detail", "group": "Fractal Noise", "name": "Fractal Detail", "type": "slider", "default": 25, "min": 10, "max": 100, "decimals": 1},
    {"id": "source_opacity", "name": "Source Opacity", "type": "slider", "default": 100, "min": 0, "max": 100, "decimals": 1},
    {"id": "shine_opacity", "name": "Shine Opacity", "type": "slider", "default": 100, "min": 0, "max": 100, "decimals": 1},
    {"id": "blend", "name": "Blend Mode", "type": "popup", "options": ["None", "Normal", "Add", "Screen", "Multiply", "Overlay", "Soft Light", "Hard Light", "Color Dodge", "Color Burn", "Darken", "Lighten", "Difference", "Exclusion"], "default": 3},
    {"id": "extend", "name": "Extend Past Layer", "type": "checkbox", "default": false}
  ]
}"#;
/// Flattened parameter count (points take 2, colours 4).
pub const NPARAMS: usize = 60;

/// The working grid's long side (the blurred light, shimmer, colorize; upsampled at the end).
/// Natively the rays sample the emission at full resolution and land on a 960 grid: 3–8 px streaks
/// across the rays at 90% / 67% of the reference's strength (radius 700 / 900; 104% / 78% with
/// everything at full resolution) in ~12.6 ms a 1080p frame.
/// In the WebAssembly interpreter every pixel costs ~100x more, so emission and rays use 480.
const WORK_SIZE: usize = if cfg!(feature = "par") { 960 } else { 480 };
#[cfg(test)]
const PASSES: i32 = 3;
#[cfg(test)]
const TAPS: usize = 8;
/// Ray falloff, measured against reference renders at Ray Length 1, 2, 4 and 8: a
/// pixel at distance d from the source averages the emission at distances s·d, weighted
/// e^(RAY_DECAY · u / Ray Length) with u = ln s (rms error under 0.005 at every length). In the
/// blur's log steps that weight composes exactly across the passes.
const RAY_DECAY: f32 = 9.5;
/// Where the rays stop: weight e^-RAY_CUT (and never past s = e^-7).
const RAY_CUT: f32 = 7.0;
/// The shimmer's travel distance uses the s^SHIMMER_MOMENT moment of the light, which leans on
/// the nearest light along the ray, as the reference's streaks do (they grow from the light's edge).
const SHIMMER_MOMENT: i32 = 8;
/// Ray brightness at Boost Light 0 (Boost multiplies by 1 + boost); 1 matches the reference.
const GAIN: f32 = 1.0;
/// Where each Colorize gradient's colours sit on the ray brightness, measured from the
/// reference renders: below the first stop the first colour, above the last the last.
const STOPS3: [f32; 3] = [0.24, 0.78, 1.0];
const STOPS5: [f32; 5] = [0.22, 0.47, 0.63, 0.86, 1.0];
// Shimmer and fractal noise, calibrated against reference renders:
/// Shimmer noise cells per revolution of Phase (reference: correlation 0.48 at 45°, 0.30 at 90°).
const PHASE_CELLS: f32 = 3.2;
/// Shimmer noise cells around the source: SHIMMER_CELLS · Detail^0.88 (reference: 0.5-correlation
/// ≈ 49° / Detail^0.88).
const SHIMMER_CELLS: f32 = 5.0;
/// Start the phase and evolution off the noise lattice, where value noise barely changes.
const NOISE_OFFSET: f32 = 0.37;
/// Shimmer strength: modulation std per unit Amount per layer pixel the light travelled from
/// where it was emitted (reference: std 0.20 at Amount 200, 200 px from the light).
const SHIMMER_K: f32 = 4.6e-6;
/// The reference's shimmer is ~13% stronger than SHIMMER_K alone at Detail 3 and up, and weaker at Detail
/// 1–2: SHIMMER_GAIN · min(1, (Detail/3)^0.4) matches the mean over 8 noise patterns per Detail
/// (reference/ours 0.77, 0.93, 1.2, 1.13 at Detail 1, 2, 3, 10).
const SHIMMER_GAIN: f32 = 1.13;
/// Std of the shimmer noise mix (one octave + half an octave up, each normalised), to make it 1.
const SHIMMER_STD: f32 = 0.19;
/// Std of one normalised octave (Reduce Flickering).
const NOISE_STD: f32 = 0.217;
/// Fractal noise cell size in layer pixels per unit of effective size Size / (1 + Size/150),
/// across and down: the reference's noise is stretched 1.75:1 and grows sub-linearly with Size.
const FRACTAL_CELL: [f32; 2] = [2.88, 1.62];
/// Fractal X/Y Speed in layer pixels per second per unit.
const FRACTAL_SPEED: f32 = 4.0;
/// Noise cells of evolution per second per unit of Evolution Speed (reference at 5: correlation
/// 0.72 after 0.5 s).
const FRACTAL_EVO: f32 = 0.112;
/// The reference's noise mean and our noise's std relative to its (0.207 vs 0.214, one octave).
const FRACTAL_MEAN: f32 = 0.485;
const FRACTAL_STD_SCALE: f32 = 0.95;

pub type Px = [f32; 4];

pub struct Params {
    threshold: f32,
    use_mask: bool,
    mask_radius: f32,
    mask_feather: f32,
    source: [f32; 2],
    ray_length: f32,
    sh_amount: f32,
    sh_detail: f32,
    sh_source: bool,
    sh_radius: f32,
    sh_smooth: bool,
    sh_phase: f32,
    sh_loop: bool,
    sh_revs: f32,
    boost: f32,
    colorize: u32,
    base_on: u32,
    /// Shadows, Mid Low, Midtones, Mid High, Highlights.
    colors: [[f32; 3]; 5],
    edge: f32,
    fr_on: bool,
    fr_center: [f32; 2],
    fr_speed: [f32; 2],
    fr_evo: f32,
    fr_bright: f32,
    fr_contrast: f32,
    fr_opacity: f32,
    fr_size: f32,
    fr_octaves: u32,
    fr_rot: f32,
    fr_mask: bool,
    fr_mask_radius: f32,
    fr_mask_feather: f32,
    fr_blend: u32,
    fr_detail: f32,
    src_opacity: f32,
    shine_opacity: f32,
    blend: u32,
    extend: bool,
    /// The layer's long side in buffer pixels when it is padded (`extend`): the working size
    /// follows the layer, not the padded buffer, so the rays stay as sharp. 0: the buffer's.
    pub layer_long: f32,
}

impl Params {
    pub fn from_flat(p: &[f64]) -> Self {
        let f = |i: usize| p[i] as f32;
        let b = |i: usize| p[i] != 0.0;
        let c = |i: usize| [f(i), f(i + 1), f(i + 2)];
        let u = |i: usize| p[i].round().max(0.0) as u32;
        Params {
            threshold: (f(0) / 255.0).clamp(0.0, 1.0),
            use_mask: b(1),
            mask_radius: f(2).max(0.0),
            mask_feather: f(3).max(0.0),
            source: [f(4), f(5)],
            ray_length: f(6).max(0.0),
            sh_amount: f(7).max(0.0) / 100.0,
            sh_detail: f(8).max(0.0),
            sh_source: b(9),
            sh_radius: f(10).max(1.0),
            sh_smooth: b(11),
            sh_phase: f(12) / 360.0,
            sh_loop: b(13),
            sh_revs: f(14).round().max(1.0),
            boost: f(15).max(0.0),
            colorize: u(16),
            base_on: u(17),
            colors: [c(34), c(30), c(26), c(22), c(18)],
            edge: f(38).max(1.0),
            fr_on: b(39),
            fr_center: [f(40), f(41)],
            fr_speed: [f(42), f(43)],
            fr_evo: f(44),
            fr_bright: f(45).max(0.0) / 100.0,
            fr_contrast: f(46).max(0.0) / 100.0,
            fr_opacity: (f(47) / 100.0).clamp(0.0, 1.0),
            fr_size: f(48).max(0.1),
            fr_octaves: u(49).clamp(1, 10),
            fr_rot: f(50).to_radians(),
            fr_mask: b(51),
            fr_mask_radius: f(52).max(0.0),
            fr_mask_feather: f(53).max(0.0),
            fr_blend: u(54),
            fr_detail: f(55).max(1.0),
            src_opacity: (f(56) / 100.0).clamp(0.0, 1.0),
            shine_opacity: (f(57) / 100.0).clamp(0.0, 1.0),
            blend: u(58),
            extend: b(59),
            layer_long: 0.0,
        }
    }
}

fn luma(c: &[f32]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// 1 inside radius `r`, 0 outside, a smooth ramp `f` wide centred on the edge.
fn circle(d: f32, r: f32, f: f32) -> f32 {
    if f <= 0.0 {
        (d <= r) as u8 as f32
    } else {
        smooth(((r + f * 0.5 - d) / f).clamp(0.0, 1.0))
    }
}

fn hash(x: i32, y: i32, z: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ (z as u32).wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Value noise in [0, 1); an x or y axis with period > 0 wraps (seamless loops).
fn vnoise(x: f32, y: f32, z: f32, px: i32, py: i32) -> f32 {
    let wrap = |i: i32, p: i32| if p > 0 { i.rem_euclid(p) } else { i };
    let (xf, yf, zf) = (x.floor(), y.floor(), z.floor());
    let t = [smooth(x - xf), smooth(y - yf), smooth(z - zf)];
    let (xi, yi, zi) = (xf as i32, yf as i32, zf as i32);
    let mut acc = 0.0;
    for dz in 0..2 {
        for dy in 0..2 {
            for dx in 0..2 {
                let wt = |d: i32, t: f32| if d == 1 { t } else { 1.0 - t };
                acc += wt(dx, t[0]) * wt(dy, t[1]) * wt(dz, t[2]) * hash(wrap(xi + dx, px), wrap(yi + dy, py), zi + dz);
            }
        }
    }
    acc
}

/// Value noise between two lattice planes is a blend of two independent values, with less spread
/// than on a plane. Dividing by the blend's own spread keeps the noise's contrast steady along an
/// animated axis (evolution, shimmer phase) instead of breathing as it crosses the lattice.
fn blend_norm(t: f32) -> f32 {
    let s = smooth(t - t.floor());
    1.0 / ((1.0 - s) * (1.0 - s) + s * s).sqrt()
}

/// Octaves weighted persistence^k, normalised as if the persistence were 0.5 (the calibrated
/// default), so a higher persistence adds fine detail without dimming the coarse octave (as the
/// reference's Fractal Detail does).
fn fbm(x: f32, y: f32, z: f32, octaves: u32, persistence: f32) -> f32 {
    let (mut amp, mut freq, mut sum, mut norm, mut half) = (1.0, 1.0, 0.0, 0.0, 1.0);
    for o in 0..octaves {
        sum += amp * (vnoise(x * freq + o as f32 * 31.7, y * freq, z * freq, 0, 0) - 0.5) * blend_norm(z * freq);
        norm += half;
        amp *= persistence;
        half *= 0.5;
        freq *= 2.0;
    }
    0.5 + sum / norm
}

/// Runs `f(y, row)` over the `len`-wide rows of `buf`: on every core in the native helper
/// (feature `par`), one after another in WebAssembly.
fn rows<T: Send>(buf: &mut [T], len: usize, f: impl Fn(usize, &mut [T]) + Sync + Send) {
    #[cfg(feature = "par")]
    {
        use rayon::prelude::*;
        buf.par_chunks_mut(len).enumerate().for_each(|(y, r)| f(y, r));
    }
    #[cfg(not(feature = "par"))]
    buf.chunks_mut(len).enumerate().for_each(|(y, r)| f(y, r));
}

/// Blurs `a` toward (cx, cy): scales about the source compose, so passes with log steps U/8,
/// U/64, U/512 cover the whole ray [e^U, 1] evenly with 512 samples for 24 taps; weights e^(decay·u).
/// The last channel is also weighted by s^SHIMMER_MOMENT (s = the sample's fraction of the
/// distance to the source); s multiplies across passes like the weights do, so it ends up as
/// Σ w·s^m·light exactly.
/// With `moment` false every channel is an ordinary ray.
/// The reference the tests hold [`polar_blur`] to.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn radial_blur<const N: usize>(out: &mut [[f32; N]], dw: usize, dh: usize, cx: f32, cy: f32, u_total: f32, decay: f32, moment: bool) {
    // Ping-pong buffers with a one-pixel zero border, so a bilinear sample half outside the
    // image needs no special case (it reads the zeros the original treated the outside as).
    let (pw, ph) = (dw + 2, dh + 2);
    let mut a = vec![[0.0f32; N]; pw * ph];
    let mut b = vec![[0.0f32; N]; pw * ph];
    for (y, row) in out.chunks_exact(dw).enumerate() {
        a[(y + 1) * pw + 1..(y + 1) * pw + 1 + dw].copy_from_slice(row);
    }
    for k in 0..PASSES {
        let du = u_total / (TAPS as f32).powi(k + 1);
        let mut sc = [0.0f32; TAPS];
        let mut wt = [0.0f32; TAPS];
        for j in 0..TAPS {
            let u = j as f32 * du;
            sc[j] = u.exp();
            wt[j] = (decay * u).exp();
        }
        let sum: f32 = wt.iter().sum();
        wt.iter_mut().for_each(|v| *v /= sum);
        let wm: [f32; TAPS] = std::array::from_fn(|j| if moment { wt[j] * sc[j].powi(SHIMMER_MOMENT) } else { wt[j] });
        let src = &a;
        rows(&mut b[pw..pw * (dh + 1)], pw, |y, prow| {
            let a = src;
            // Along a row each tap reads the same two source rows (y only scales toward cy), so
            // the vertical position, weight and bounds are per row; only x moves per pixel.
            let qy = y as f32 + 0.5 - cy;
            let mut rows = [(0usize, 0.0f32, 0.0f32); TAPS];
            let mut live = [false; TAPS];
            for j in 0..TAPS {
                let fy = cy + qy * sc[j] - 0.5;
                let y0 = fy.floor();
                // Padded row y0 + 1 and the one below it; outside them the sample is 0.
                live[j] = y0 >= -1.0 && y0 <= dh as f32 - 1.0;
                rows[j] = (((y0 + 1.0).max(0.0) as usize) * pw, fy - y0, cx * (1.0 - sc[j]) - 0.5 + 1.0);
            }
            for (x, o) in prow[1..1 + dw].iter_mut().enumerate() {
                let qx = x as f32 + 0.5;
                let mut acc = [0.0f32; N];
                for j in 0..TAPS {
                    if !live[j] {
                        continue;
                    }
                    let (r0, ty, off) = rows[j];
                    // Padded column of the left neighbour: (cx + (qx − cx)·sc − 0.5) + 1.
                    let fx = off + qx * sc[j];
                    let x0 = fx.floor();
                    if !(0.0..=dw as f32).contains(&x0) {
                        continue;
                    }
                    let tx = fx - x0;
                    let i = r0 + x0 as usize;
                    // SAFETY: rows 0..=dh and columns 0..=dw of the (dw+2)×(dh+2) buffer, so
                    // i + pw + 1 < pw · ph.
                    let (p00, p01, p10, p11) = unsafe { (a.get_unchecked(i), a.get_unchecked(i + 1), a.get_unchecked(i + pw), a.get_unchecked(i + pw + 1)) };
                    for c in 0..N {
                        let top = p00[c] + (p01[c] - p00[c]) * tx;
                        let bot = p10[c] + (p11[c] - p10[c]) * tx;
                        acc[c] += (top + (bot - top) * ty) * if c == N - 1 { wm[j] } else { wt[j] };
                    }
                }
                *o = acc;
            }
        });
        std::mem::swap(&mut a, &mut b);
    }
    for (y, row) in out.chunks_exact_mut(dw).enumerate() {
        row.copy_from_slice(&a[(y + 1) * pw + 1..(y + 1) * pw + 1 + dw]);
    }
}

/// The ray blur, computed along rays (the 3-pass tap blur it replaced, `radial_blur`, is kept as
/// the tests' reference): in polar coordinates around the source a sample at radius r averages the
/// light at r' ∈ [r·e^U, r] weighted (r'/r)^decay dr'/r', which a running sum along each ray gives
/// exactly (g(r) = ∫₀ʳ (r'/r)^decay L dr'/r', and the cut is g(r) − e^(decay·U)·g(r·e^U)).
///
/// `src` is the emission at the fine resolution (`fw`×`fh`, a 2-pixel zero border), which the rays
/// sample every fine pixel; they are one working pixel apart at the farthest corner, and `out` is
/// the working grid (`f` fine pixels per working pixel). Sampling the fine emission keeps the
/// angular detail of light near the source, which the streaks magnify on their way out. Channel N
/// of `out` is the light's sum, weighted by s^SHIMMER_MOMENT when `moment` (for shimmer).
#[allow(clippy::too_many_arguments)]
fn polar_blur<const N: usize, const M: usize>(src: &[[f32; N]], fw: usize, fh: usize, f: f32, out: &mut [[f32; M]], dw: usize, cx: f32, cy: f32, u_total: f32, decay: f32, moment: bool) {
    debug_assert_eq!(M, N + 1);
    let r_max = [(0.0, 0.0), (fw as f32, 0.0), (0.0, fh as f32), (fw as f32, fh as f32)].iter().map(|(x, y)| (x - cx).hypot(y - cy)).fold(0.0f32, f32::max);
    let nr = r_max.ceil() as usize + 2;
    let na = ((TAU * r_max / f).ceil() as usize).max(8);
    // Per radius step k (r_{k−1} → r_k): carry factor (r_{k−1}/r_k)^d and the weight of the
    // interval's light, ∫ (r'/r_k)^d dr'/r' = (1 − carry)/d; the same for every ray.
    let dm = if moment { decay + SHIMMER_MOMENT as f32 } else { decay };
    let steps = |d: f32| -> Vec<(f32, f32)> { (0..nr).map(|k| if k == 0 { (0.0, 0.0) } else { let c = ((k - 1) as f32 / k as f32).powf(d); (c, (1.0 - c) / d) }).collect() };
    let (st, stm) = (steps(decay), steps(dm));
    let (cut, cutm) = ((decay * u_total).exp(), (dm * u_total).exp());
    let inv_z = decay / (1.0 - cut);
    let shrink = u_total.exp();
    let pw = fw + 4;
    // Not zeroed between frames: every value the output reads is written below.
    let mut store = POLAR.with(|b| std::mem::take(&mut *b.borrow_mut()));
    if store.len() < na * nr * M {
        store.resize(na * nr * M, 0.0);
    }
    let (polar, _) = store[..na * nr * M].as_chunks_mut::<M>();
    // Samples at r = k − ½ stay within [−1, n + 1] on both axes (the padded image) for k < lit(a);
    // past that the ray has left the image and there is no light.
    let lit = |a: usize| {
        let (sn, cs) = (a as f32 * TAU / na as f32).sin_cos();
        let exit = |c: f32, d: f32, n: usize| if d > 1e-9 { (n as f32 + 1.0 - c) / d } else if d < -1e-9 { (c + 1.0) / -d } else { f32::INFINITY };
        ((exit(cx, cs, fw).min(exit(cy, sn, fh)) + 0.5).max(0.0).floor() as usize).min(nr)
    };
    rows(polar, nr, |a, ray| {
        let (sn, cs) = (a as f32 * TAU / na as f32).sin_cos();
        let lit_a = lit(a);
        // The output reads this ray only for pixels between it and its neighbours, all inside the
        // image: stop a few steps past where the three leave it (between two rays the image's edge
        // can be up to a ray spacing, f fine pixels, farther out).
        let end = (lit_a.max(lit((a + na - 1) % na)).max(lit((a + 1) % na)) + 3 + f.ceil() as usize).min(nr);
        let mut g = [0.0f32; M];
        ray[0] = g;
        for k in 1..end {
            let l: [f32; N] = if k < lit_a {
                // Padded coordinates of the sample's top-left neighbour: (c + r·dir − 0.5) + 2.
                let r = k as f32 - 0.5;
                let (fx, fy) = (cx + r * cs + 1.5, cy + r * sn + 1.5);
                let (x0, y0) = (fx as usize, fy as usize);
                let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                let i = y0 * pw + x0;
                let (p00, p01, p10, p11) = (src[i], src[i + 1], src[i + pw], src[i + pw + 1]);
                std::array::from_fn(|c| {
                    let top = p00[c] + (p01[c] - p00[c]) * tx;
                    let bot = p10[c] + (p11[c] - p10[c]) * tx;
                    top + (bot - top) * ty
                })
            } else {
                [0.0; N]
            };
            let (carry, w) = st[k];
            for c in 0..N {
                g[c] = g[c] * carry + l[c] * w;
            }
            let (carry, w) = stm[k];
            g[N] = g[N] * carry + l.iter().sum::<f32>() * w;
            ray[k] = g;
        }
        // Cut each ray at r·e^U (top down, so the g below k is still intact) and normalise.
        for k in (1..end).rev() {
            let x = k as f32 * shrink;
            let k0 = x as usize;
            let t = x - k0 as f32;
            let (lo, hi) = (ray[k0], ray[(k0 + 1).min(k)]);
            for c in 0..M {
                let below = lo[c] + (hi[c] - lo[c]) * t;
                ray[k][c] = (ray[k][c] - below * if c == N { cutm } else { cut }) * inv_z;
            }
        }
    });
    rows(out, dw, |y, row| {
        let dy = (y as f32 + 0.5) * f - cy;
        for (x, o) in row.iter_mut().enumerate() {
            let dx = (x as f32 + 0.5) * f - cx;
            let fk = (dx * dx + dy * dy).sqrt().min((nr - 2) as f32);
            let fa = atan2_fast(dy, dx).rem_euclid(TAU) * (na as f32 / TAU);
            let (k0, a0) = (fk as usize, (fa as usize).min(na - 1));
            let (tk, ta) = (fk - k0 as f32, fa - a0 as f32);
            let a1 = (a0 + 1) % na;
            let (p00, p01, p10, p11) = (polar[a0 * nr + k0], polar[a0 * nr + k0 + 1], polar[a1 * nr + k0], polar[a1 * nr + k0 + 1]);
            for c in 0..M {
                let near = p00[c] + (p01[c] - p00[c]) * tk;
                let far = p10[c] + (p11[c] - p10[c]) * tk;
                o[c] = near + (far - near) * ta;
            }
        }
    });
    POLAR.with(|b| *b.borrow_mut() = store);
}

/// The fine emission box-averaged onto the working grid (`f` fine pixels per working pixel), plus
/// its channel sum: the light where there are no rays (Ray Length 0).
fn box_to_work<const N: usize, const M: usize>(src: &[[f32; N]], fw: usize, fh: usize, f: usize, out: &mut [[f32; M]], dw: usize) {
    let pw = fw + 4;
    rows(out, dw, |y, row| {
        for (x, o) in row.iter_mut().enumerate() {
            let mut acc = [0.0f32; M];
            for fy in y * f..((y + 1) * f).min(fh) {
                for fx in x * f..((x + 1) * f).min(fw) {
                    let v = src[(fy + 2) * pw + fx + 2];
                    for c in 0..N {
                        acc[c] += v[c];
                        acc[N] += v[c];
                    }
                }
            }
            *o = acc.map(|v| v / (f * f) as f32);
        }
    });
}

thread_local! {
    // The two big buffers (fine emission, rays), kept between frames: a fresh multi-megabyte
    // buffer costs a page fault per 4 KB.
    static POLAR: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
    static PADDED: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
}

/// atan2 to about 1e-5 rad (0.01 px at 1000 px from the source); the library one dominated the
/// blur's per-pixel cost.
fn atan2_fast(y: f32, x: f32) -> f32 {
    use std::f32::consts::{FRAC_PI_2, PI};
    let (ax, ay) = (x.abs(), y.abs());
    let (lo, hi) = (ax.min(ay), ax.max(ay));
    if hi == 0.0 {
        return 0.0;
    }
    let a = lo / hi;
    let s = a * a;
    let mut r = (((-0.046_496_473 * s + 0.159_314_22) * s - 0.327_622_76) * s) * a + a;
    if ay > ax {
        r = FRAC_PI_2 - r;
    }
    if x < 0.0 {
        r = PI - r;
    }
    if y < 0.0 { -r } else { r }
}

/// Bilinear upsample positions for one axis: (i0, i1, t) per output pixel, clamped at the edges.
fn axis(n_out: usize, n_in: usize, inv: f32) -> Vec<(usize, usize, f32)> {
    (0..n_out)
        .map(|o| {
            let f = ((o as f32 + 0.5) * inv - 0.5).clamp(0.0, (n_in - 1) as f32);
            let i0 = f as usize;
            (i0, (i0 + 1).min(n_in - 1), f - i0 as f32)
        })
        .collect()
}

/// The colour at brightness `v` of a gradient with `cols` at positions `pos` (ascending).
fn gradient(pos: &[f32], cols: &[[f32; 3]], v: f32) -> [f32; 3] {
    let i = pos.iter().position(|&p| v < p).unwrap_or(pos.len());
    if i == 0 {
        return cols[0];
    }
    if i == pos.len() {
        return cols[pos.len() - 1];
    }
    let f = (v - pos[i - 1]) / (pos[i] - pos[i - 1]);
    let (a, b) = (cols[i - 1], cols[i]);
    [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f]
}

/// Separable blend of source `cb` and shine `cs` (both 0–1), modes 3.. of the Blend Mode popup.
fn blend_channel(mode: u32, cb: f32, cs: f32) -> f32 {
    let screen = |a: f32, b: f32| a + b - a * b;
    let hard = |cb: f32, cs: f32| if cs <= 0.5 { cb * 2.0 * cs } else { screen(cb, 2.0 * cs - 1.0) };
    match mode {
        4 => cb * cs,
        5 => hard(cs, cb),
        6 => {
            if cs <= 0.5 {
                cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
            } else {
                let d = if cb <= 0.25 { ((16.0 * cb - 12.0) * cb + 4.0) * cb } else { cb.sqrt() };
                cb + (2.0 * cs - 1.0) * (d - cb)
            }
        }
        7 => hard(cb, cs),
        8 => {
            if cb <= 0.0 {
                0.0
            } else if cs >= 1.0 {
                1.0
            } else {
                (cb / (1.0 - cs)).min(1.0)
            }
        }
        9 => {
            if cb >= 1.0 {
                1.0
            } else if cs <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - cb) / cs).min(1.0)
            }
        }
        10 => cb.min(cs),
        11 => cb.max(cs),
        12 => (cb - cs).abs(),
        13 => cb + cs - 2.0 * cb * cs,
        _ => screen(cb, cs),
    }
}

/// Source `b` and shine `s`, both premultiplied.
fn composite(b: Px, s: Px, mode: u32) -> Px {
    let a = b[3] + s[3] - b[3] * s[3];
    match mode {
        0 => s,
        1 => [s[0] + b[0] * (1.0 - s[3]), s[1] + b[1] * (1.0 - s[3]), s[2] + b[2] * (1.0 - s[3]), a],
        2 => [b[0] + s[0], b[1] + s[1], b[2] + s[2], a],
        _ => {
            // ponytail: the blend runs on colours clamped to 0–1, so 32 bpc overbrights clip in
            // these modes (Add and Normal keep them).
            let inv = |a: f32| if a > 1e-6 { 1.0 / a } else { 0.0 };
            let (ib, is) = (inv(b[3]), inv(s[3]));
            let mut o = [0.0, 0.0, 0.0, a];
            for i in 0..3 {
                let m = blend_channel(mode, (b[i] * ib).clamp(0.0, 1.0), (s[i] * is).clamp(0.0, 1.0));
                o[i] = s[i] * (1.0 - b[3]) + b[i] * (1.0 - s[3]) + s[3] * b[3] * m;
            }
            o
        }
    }
}

/// Renders the effect in place on premultiplied RGBA pixels. `time` is layer time in seconds,
/// `scale` buffer pixels per layer pixel.
pub fn shine(px: &mut [Px], w: usize, h: usize, p: &Params, time: f32, scale: f32) {
    if w == 0 || h == 0 {
        return;
    }
    let scale = scale.max(1e-6);
    let long = if p.layer_long >= 1.0 { (p.layer_long as usize).min(w.max(h)) } else { w.max(h) };
    let ds = long.div_ceil(WORK_SIZE).max(1);
    let dsf = ds as f32;
    let (dw, dh) = (w.div_ceil(ds), h.div_ceil(ds));

    // 1. The fine grid the rays sample: the frame itself natively, the working grid in WebAssembly
    // (pixels box-averaged in fs×fs blocks; every pixel costs ~100x more in the interpreter).
    // ponytail: edge blocks are averaged over a full block, so the last row/column is slightly
    // dimmer when the size isn't a multiple of fs.
    let fs = if cfg!(feature = "par") { 1 } else { ds };
    let (fw, fh) = (w.div_ceil(fs), h.div_ceil(fs));
    let fsf = fs as f32;
    let boxed: Option<Vec<Px>> = (fs > 1).then(|| {
        let mut a = vec![[0.0f32; 4]; fw * fh];
        let src: &[Px] = px;
        rows(&mut a, fw, |ya, arow| {
            for row in src[ya * fs * w..((ya + 1) * fs).min(h) * w].chunks_exact(w) {
                for (d, blk) in arow.iter_mut().zip(row.chunks(fs)) {
                    // Sums in locals: the interpreter pays for every load and store of `d`.
                    let (mut r, mut g, mut b, mut al) = (0.0, 0.0, 0.0, 0.0);
                    for c in blk {
                        (r, g, b, al) = (r + c[0], g + c[1], b + c[2], al + c[3]);
                    }
                    *d = [d[0] + r, d[1] + g, d[2] + b, d[3] + al];
                }
            }
        });
        let norm = 1.0 / (fs * fs) as f32;
        a.iter_mut().for_each(|d| d.iter_mut().for_each(|v| *v *= norm));
        a
    });
    let fine: &[Px] = boxed.as_deref().unwrap_or(px);

    // Alpha Edges: alpha minus its local minimum (a square min filter, separable).
    let alpha_min = (p.base_on == 3).then(|| {
        let r = ((p.edge * scale / fsf).round() as usize).max(1);
        let mut t = vec![0.0f32; fw * fh];
        rows(&mut t, fw, |y, row| {
            for (x, o) in row.iter_mut().enumerate() {
                let (lo, hi) = (x.saturating_sub(r), (x + r).min(fw - 1));
                *o = (lo..=hi).map(|k| fine[y * fw + k][3]).fold(f32::MAX, f32::min);
            }
        });
        let mut m = vec![0.0f32; fw * fh];
        rows(&mut m, fw, |y, row| {
            let (lo, hi) = (y.saturating_sub(r), (y + r).min(fh - 1));
            for (x, o) in row.iter_mut().enumerate() {
                *o = (lo..=hi).map(|k| t[k * fw + x]).fold(f32::MAX, f32::min);
            }
        });
        m
    });

    // 2. Emission at each fine pixel, into the blur's input (a 2-pixel zero border): s = the Base
    // On value after threshold and mask. Colorized rays only need s; uncolorized ones carry the
    // pixel's colour × s/v instead.
    let colorized = p.colorize != 0;
    let t = p.threshold;
    let emit = |x: usize, y: usize| -> (f32, f32, Px) {
        let i = y * fw + x;
        let c = fine[i];
        let (bx, by) = ((x as f32 + 0.5) * fsf, (y as f32 + 0.5) * fsf);
        let v = match p.base_on {
            1 => luma(&c),
            2 => c[3],
            3 => (c[3] - alpha_min.as_ref().map_or(0.0, |m| m[i])).max(0.0),
            4 => c[0],
            5 => c[1],
            6 => c[2],
            _ => (c[0].max(c[1]).max(c[2]) + c[0].min(c[1]).min(c[2])) * 0.5,
        };
        let mut s = if v > t { (v - t) / (1.0 - t).max(1e-3) } else { 0.0 };
        if p.use_mask {
            let d = (bx - p.source[0]).hypot(by - p.source[1]);
            s *= circle(d, p.mask_radius * scale, p.mask_feather * scale);
        }
        (s, v, c)
    };
    let pw = fw + 4;
    // Kept between frames and not cleared: fill() writes every pixel and zeroes the border.
    let mut store = PADDED.with(|b| std::mem::take(&mut *b.borrow_mut()));
    let need = pw * (fh + 4) * if colorized { 1 } else { 3 };
    if store.len() < need {
        store.resize(need, 0.0);
    }
    fn fill<const N: usize>(store: &mut [f32], pw: usize, fw: usize, fh: usize, f: impl Fn(usize, usize) -> [f32; N] + Sync + Send) -> &[[f32; N]] {
        let (src, _) = store[..pw * (fh + 4) * N].as_chunks_mut::<N>();
        src[..2 * pw].fill([0.0; N]);
        src[(fh + 2) * pw..].fill([0.0; N]);
        rows(&mut src[2 * pw..(fh + 2) * pw], pw, |y, row| {
            row[..2].fill([0.0; N]);
            row[2 + fw..].fill([0.0; N]);
            row[2..2 + fw].iter_mut().enumerate().for_each(|(x, o)| *o = f(x, y));
        });
        &*src
    }

    // 3. Radial blur toward the source, onto the working grid: [light, moment] or [r, g, b,
    // moment] per working pixel. The moment (the light weighted by s^SHIMMER_MOMENT) is only
    // computed for shimmer; without it the last channel is the light's plain sum.
    let n = dw * dh;
    let mut lum: Vec<[f32; 2]> = if colorized { vec![[0.0; 2]; n] } else { Vec::new() };
    let mut hue: Vec<[f32; 4]> = if colorized { Vec::new() } else { vec![[0.0; 4]; n] };
    let (fcx, fcy) = (p.source[0] / fsf, p.source[1] / fsf);
    // The source on the working grid (shimmer's angles).
    let (cx, cy) = (p.source[0] / dsf, p.source[1] / dsf);
    let shimmer = p.sh_amount > 0.0 && p.sh_detail > 0.0;
    let f = ds / fs;
    let decay = RAY_DECAY / p.ray_length.max(1e-6);
    let u_total = -(RAY_CUT / decay).min(7.0);
    if colorized {
        let src = fill::<1>(&mut store, pw, fw, fh, |x, y| [emit(x, y).0]);
        if p.ray_length > 0.0 {
            polar_blur(src, fw, fh, f as f32, &mut lum, dw, fcx, fcy, u_total, decay, shimmer);
        } else {
            box_to_work(src, fw, fh, f, &mut lum, dw);
        }
    } else {
        // The pixel's own colour, scaled by what the threshold and mask left of it.
        let src = fill::<3>(&mut store, pw, fw, fh, |x, y| {
            let (s, v, c) = emit(x, y);
            let k = if v > 1e-6 { s / v } else { 0.0 };
            if v > 1e-6 { [c[0] * k, c[1] * k, c[2] * k] } else { [s; 3] }
        });
        if p.ray_length > 0.0 {
            polar_blur(src, fw, fh, f as f32, &mut hue, dw, fcx, fcy, u_total, decay, shimmer);
        } else {
            box_to_work(src, fw, fh, f, &mut hue, dw);
        }
    }
    PADDED.with(|b| *b.borrow_mut() = store);
    let mut a = vec![[0.0f32; 4]; n];

    // 4. Shimmer, boost, colorize → premultiplied shine (reusing the downsample buffer).
    // Shimmer: angular noise that varies each ray in proportion to how far its light travelled
    // (the streaks are faint at the light and grow with distance from it).
    let gain = GAIN * (1.0 + p.boost);
    let cells = (SHIMMER_CELLS * p.sh_detail.powf(0.88)).round().max(1.0) as i32;
    // A loop wraps the noise after a whole number of cells, so its phase rate is fitted to that.
    let period = if p.sh_loop { (p.sh_revs * PHASE_CELLS).round().max(1.0) as i32 } else { 0 };
    let rate = if p.sh_loop { period as f32 / p.sh_revs } else { PHASE_CELLS };
    let ph = p.sh_phase * rate + NOISE_OFFSET;
    // ponytail: Source Point affects Shimmer / Radius / Reduce Flickering made no measurable
    // difference in the reference renders; here the source position (in units of Radius) offsets the
    // pattern, so it changes as the source moves, and Reduce Flickering drops the fine octave.
    let ang_off = if p.sh_source { (p.source[0] * 0.37 + p.source[1] * 0.61) / (p.sh_radius * scale).max(1.0) } else { 0.0 };
    let sh_k = SHIMMER_K * SHIMMER_GAIN * (p.sh_detail / 3.0).powf(0.4).min(1.0) * p.sh_amount * 100.0;
    let px_layer = dsf / scale;
    let three = [p.colors[0], p.colors[2], p.colors[4]];
    // Fractal noise: applied to the rays at each pixel, like fog the light passes through
    // (in the reference renders the ratio with/without noise correlates 0.84 with the noise at the
    // pixel and 0.12 with the noise where the light was emitted).
    let size = p.fr_size / (1.0 + p.fr_size / 150.0) * scale;
    let cell = [FRACTAL_CELL[0] * size, FRACTAL_CELL[1] * size];
    let (fsin, fcos) = p.fr_rot.sin_cos();
    let drift = FRACTAL_SPEED * scale * time;
    let fr_origin = [p.fr_center[0] + p.fr_speed[0] * drift, p.fr_center[1] + p.fr_speed[1] * drift];
    let evo = NOISE_OFFSET + time * p.fr_evo * FRACTAL_EVO;
    // Fractal Detail: the fine octaves' persistence, 0.5 at the default 25. Fitted to the reference's power
    // per octave (x0.72 / x0.57 at octaves 1 / 2 for Detail 10, x1.05 / x1.25
    // for Detail 100).
    let fr_persist = 0.56 * p.fr_detail / (p.fr_detail + 3.0);
    let fog = |x: usize, y: usize| -> (f32, f32) {
        let (bx, by) = ((x as f32 + 0.5) * dsf, (y as f32 + 0.5) * dsf);
        let (dx, dy) = (bx - fr_origin[0], by - fr_origin[1]);
        let (nx, ny) = ((dx * fcos + dy * fsin) / cell[0], (dy * fcos - dx * fsin) / cell[1]);
        let n = FRACTAL_MEAN + (fbm(nx, ny, evo, p.fr_octaves, fr_persist) - 0.5) * FRACTAL_STD_SCALE;
        let n = (((n - 0.5) * p.fr_contrast + 0.5) * p.fr_bright).clamp(0.0, 1.0);
        let mut o = p.fr_opacity;
        if p.fr_mask {
            o *= circle((bx - p.fr_center[0]).hypot(by - p.fr_center[1]), p.fr_mask_radius * scale, p.fr_mask_feather * scale);
        }
        (n, o)
    };
    let apply_fog = |s: f32, n: f32, o: f32| match p.fr_blend {
        1 => s * (1.0 - o + o * n),
        2 => s + s * n * o - s * s * n * o,
        3 => s + o * (s.max(n) - s),
        _ => s * (1.0 + o * n),
    };
    rows(&mut a, dw, |y, arow| {
        for (x, out) in arow.iter_mut().enumerate() {
            let i = y * dw + x;
            let (light, moment) = if colorized { (lum[i][0], lum[i][1]) } else { (hue[i][0] + hue[i][1] + hue[i][2], hue[i][3]) };
            let mut k = gain;
            if shimmer && light > 1e-6 {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let ang = (dy.atan2(dx) / TAU + 0.5) * cells as f32 + ang_off;
                let n1 = (vnoise(ang, ph, 0.0, cells, period) - 0.5) * blend_norm(ph);
                let nz = if p.sh_smooth {
                    n1 / NOISE_STD
                } else {
                    let n2 = (vnoise(ang * 2.0, ph * 2.0, 0.0, cells * 2, period * 2) - 0.5) * blend_norm(ph * 2.0);
                    (n1 + 0.5 * n2) / 1.5 / SHIMMER_STD
                };
                let travel = dx.hypot(dy) * px_layer * (1.0 - (moment / light).max(0.0).powf(1.0 / SHIMMER_MOMENT as f32)).max(0.0);
                // Exponential: log m is symmetric with a std ∝ travel, as measured.
                k *= (sh_k * nz * travel).min(20.0).exp();
            }
            // The fogged ray brightness (before colorizing, so the clouds shift the colours too).
            let (fn_, fo) = if p.fr_on { fog(x, y) } else { (0.0, 0.0) };
            let f = |s: f32| if p.fr_on { apply_fog(s, fn_, fo) } else { s };
            let rgb = match p.colorize {
                0 => [f(hue[i][0] * k), f(hue[i][1] * k), f(hue[i][2] * k)],
                1 => p.colors[2].map(|c| c * f(light * k)),
                2 => {
                    let s = f(light * k);
                    gradient(&STOPS3, &three, s).map(|c| c * s)
                }
                _ => {
                    let s = f(light * k);
                    gradient(&STOPS5, &p.colors, s).map(|c| c * s)
                }
            };
            let al = rgb[0].max(rgb[1]).max(rgb[2]).clamp(0.0, 1.0);
            *out = [rgb[0], rgb[1], rgb[2], al];
        }
    });

    // 5. Upsample and blend, with the blend mode chosen once (one compiled loop per mode)
    // rather than per pixel.
    let src = p.src_opacity;
    let skip_empty = p.blend != 0 && src >= 1.0;
    macro_rules! up {
        ($f:expr) => {
            upsample_blend(px, w, h, &a, dw, dh, ds, p.shine_opacity, skip_empty, $f)
        };
    }
    match p.blend {
        0 => up!(|_, s| s),
        1 => up!(|b: Px, s| composite(b.map(|v| v * src), s, 1)),
        2 => up!(|b: Px, s| composite(b.map(|v| v * src), s, 2)),
        // Screen is exact on premultiplied colours: b + s − b·s (shine clamped to 1 per channel).
        3 => up!(|b: Px, s: Px| {
            let b = b.map(|v| v * src);
            let sc = |i: usize| s[i].min(1.0);
            [b[0] + sc(0) - b[0] * sc(0), b[1] + sc(1) - b[1] * sc(1), b[2] + sc(2) - b[2] * sc(2), b[3] + s[3] - b[3] * s[3]]
        }),
        m => up!(|b: Px, s| composite(b.map(|v| v * src), s, m)),
    }
}

/// Bilinear upsample of the work-resolution shine `a` (separable: one vertically blended work
/// row per output row) and `f(source, shine)` per pixel. `skip_empty`: leave pixels without
/// shine untouched (every mode but None does, at full source opacity).
#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn upsample_blend(px: &mut [Px], w: usize, h: usize, a: &[Px], dw: usize, dh: usize, ds: usize, opacity: f32, skip_empty: bool, f: impl Fn(Px, Px) -> Px + Sync + Send) {
    let inv = 1.0 / ds as f32;
    let ys = axis(h, dh, inv);
    // Work column k sits at output x = k·ds + (ds − 1)/2; output pixels from start(k) up to
    // start(k + 1) lie between columns k and k + 1 (clamped at both edges), where the shine is a
    // straight line, so it steps by a constant per pixel instead of being lerped from scratch.
    let start = |k: usize| (k * ds + ds / 2).min(w);
    rows(px, w, |y, out| {
        let (y0, y1, ty) = ys[y];
        let mut row = vec![[0.0f32; 4]; dw];
        let (r0, r1) = (&a[y0 * dw..(y0 + 1) * dw], &a[y1 * dw..(y1 + 1) * dw]);
        let mut any = false;
        for ((t, a0), a1) in row.iter_mut().zip(r0).zip(r1) {
            for c in 0..4 {
                t[c] = (a0[c] + (a1[c] - a0[c]) * ty) * opacity;
            }
            any |= t[3] > 0.0;
        }
        if skip_empty && !any {
            return;
        }
        // Segment k − 1 → k for k = 0 (the left edge, flat) ..= dw (the right edge, flat).
        for k in 0..=dw {
            let (i0, i1) = (k.saturating_sub(1), k.min(dw - 1));
            let (x_start, x_end) = (if k == 0 { 0 } else { start(k - 1) }, start(k).max(if k == dw { w } else { 0 }));
            let (s0, s1) = (row[i0], row[i1]);
            // Pixels without shine are left as they are (a zero shine blends to the source).
            if x_start >= x_end || (skip_empty && s0[3] <= 0.0 && s1[3] <= 0.0) {
                continue;
            }
            let t0 = if i0 == i1 { 0.0 } else { (x_start as f32 + 0.5) * inv - 0.5 - i0 as f32 };
            let step = if i0 == i1 { 0.0 } else { inv };
            let d = [0, 1, 2, 3].map(|c| (s1[c] - s0[c]) * step);
            let mut s = [0, 1, 2, 3].map(|c| s0[c] + (s1[c] - s0[c]) * t0);
            for c in &mut out[x_start..x_end] {
                *c = f(*c, s);
                for i in 0..4 {
                    s[i] += d[i];
                }
            }
        }
    });
}

/// Transparent pixels to add on each side so rays can leave a layer of `layer` size (Extend
/// Past Layer): light at distance d from the source streaks out until its weight
/// e^(RAY_DECAY · u / Ray Length) falls under 1/255, i.e. to d · e^(ln 255 · Ray Length / RAY_DECAY),
/// so the farthest layer corner sets the padding.
/// ponytail: capped at the layer's long side (the cost grows with the padded area); assumes the
/// layer starts at buffer (0, 0), i.e. no effect above padded it.
pub fn padding(p: &Params, scale: f64, layer: [f64; 2]) -> u32 {
    if !p.extend || p.ray_length <= 0.0 {
        return 0;
    }
    let (w, h) = (layer[0] * scale, layer[1] * scale);
    let (sx, sy) = (f64::from(p.source[0]), f64::from(p.source[1]));
    let far = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)].iter().map(|(x, y)| (x - sx).hypot(y - sy)).fold(0.0, f64::max);
    let reach = (255f64.ln() * f64::from(p.ray_length) / f64::from(RAY_DECAY)).min(7.0).exp() - 1.0;
    (far * reach).min(w.max(h)).ceil().clamp(0.0, 4096.0) as u32
}

// ---- WebAssembly ABI 2 (time-keyed in the render cache, which the animated noise needs) ----

thread_local! {
    static BUF: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

#[unsafe(no_mangle)]
pub extern "C" fn ec_api_version() -> i32 {
    2
}

#[unsafe(no_mangle)]
pub extern "C" fn ec_manifest_ptr() -> i32 {
    MANIFEST.as_ptr() as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn ec_manifest_len() -> i32 {
    MANIFEST.len() as i32
}

/// Bit 0: the render changes with time (shimmer, fractal evolution). Bit 1 (neighbour frames) is
/// left out, so a host that reads it doesn't render the frames either side. Older hosts ignore this.
#[unsafe(no_mangle)]
pub extern "C" fn ec_flags() -> i32 {
    1
}

/// [`padding`] for a host that asks for it; older hosts ignore it and clip the rays to the layer.
/// # Safety
/// The host passes a pointer into the buffer `ec_alloc` returned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ec_padding(params: i32, nparams: i32, _time: f64, scale: f64, layer_w: f64, layer_h: f64) -> i32 {
    if (nparams as usize) < NPARAMS {
        return 0;
    }
    let p = unsafe { std::slice::from_raw_parts(params as *const f64, nparams as usize) };
    padding(&Params::from_flat(p), scale, [layer_w, layer_h]) as i32
}

/// One reusable, 8-byte-aligned buffer for the parameters and pixels.
#[unsafe(no_mangle)]
pub extern "C" fn ec_alloc(bytes: i32) -> i32 {
    BUF.with(|b| {
        let mut b = b.borrow_mut();
        let words = (bytes.max(0) as usize).div_ceil(8);
        if b.len() < words {
            b.resize(words, 0);
        }
        b.as_mut_ptr() as i32
    })
}

/// # Safety
/// The host passes pointers into the buffer `ec_alloc` returned.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ec_render2(
    pixels: i32,
    width: i32,
    height: i32,
    params: i32,
    nparams: i32,
    time: f64,
    scale: f64,
    _prev: i32,
    _prev_time: f64,
    _next: i32,
    _next_time: f64,
    _flags: i32,
) -> i32 {
    if (nparams as usize) < NPARAMS || width <= 0 || height <= 0 {
        return 1;
    }
    let (w, h) = (width as usize, height as usize);
    let p = unsafe { std::slice::from_raw_parts(params as *const f64, nparams as usize) };
    let px = unsafe { std::slice::from_raw_parts_mut(pixels as *mut Px, w * h) };
    shine(px, w, h, &Params::from_flat(p), time as f32, scale as f32);
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The manifest's defaults, flattened as the host does (points in buffer pixels).
    fn defaults(w: f64, h: f64) -> Vec<f64> {
        let m: serde_json::Value = serde_json::from_str(MANIFEST).unwrap();
        let mut out = vec![];
        for p in m["params"].as_array().unwrap() {
            let d = &p["default"];
            match p["type"].as_str().unwrap() {
                "point" => out.extend([d[0].as_f64().unwrap() * w, d[1].as_f64().unwrap() * h]),
                "color" => out.extend(d.as_array().unwrap().iter().map(|v| v.as_f64().unwrap())),
                "checkbox" => out.push(d.as_bool().unwrap() as u8 as f64),
                _ => out.push(d.as_f64().unwrap()),
            }
        }
        out
    }

    fn spot() -> Vec<Px> {
        let mut px = vec![[0.0f32, 0.0, 0.0, 1.0]; 64 * 64];
        for y in 30..34 {
            for x in 40..44 {
                px[y * 64 + x] = [1.0; 4];
            }
        }
        px
    }

    #[test]
    fn manifest_flattens_to_nparams() {
        assert_eq!(defaults(1.0, 1.0).len(), NPARAMS);
    }

    #[test]
    fn rays_go_outward_only() {
        let mut flat = defaults(64.0, 64.0);
        flat[6] = 6.0; // ray length
        flat[58] = 2.0; // Add
        let mut px = spot();
        shine(&mut px, 64, 64, &Params::from_flat(&flat), 0.0, 1.0);
        assert!(px[32 * 64 + 56][0] > 0.01, "ray beyond the spot, away from the source");
        assert!(px[32 * 64 + 8][0] < 1e-4, "no ray on the source's far side");

        // Threshold 255: nothing is bright enough, the frame is unchanged.
        flat[0] = 255.0;
        let mut px2 = spot();
        shine(&mut px2, 64, 64, &Params::from_flat(&flat), 0.0, 1.0);
        assert_eq!(px2, spot());
    }

    /// Reference renders at the defaults on a white disc (r 24 px, 200 px right of the source,
    /// black background, 1080p): ray brightness
    /// along the ray at distance d from the source (0–255) with Colorize None, and the 3-Color
    /// Gradient's red/green there. Here at quarter size, where the working resolution is full.
    #[test]
    fn matches_reference_profile() {
        let (w, h) = (480, 270);
        let disc = || {
            let mut px = vec![[0.0f32, 0.0, 0.0, 1.0]; w * h];
            for y in 0..h {
                for x in 0..w {
                    if ((x as f32 + 0.5 - 290.0).powi(2) + (y as f32 + 0.5 - 105.0).powi(2)).sqrt() <= 6.0 {
                        px[y * w + x] = [1.0; 4];
                    }
                }
            }
            px
        };
        let mut flat = defaults(w as f64, h as f64);
        flat[4] = 240.0; // source
        flat[5] = 105.0;
        let at = |px: &[Px], d: f32| px[105 * w + (240.0 + d / 4.0) as usize];

        flat[16] = 0.0; // Colorize None
        let mut none = disc();
        shine(&mut none, w, h, &Params::from_flat(&flat), 0.0, 1.0);
        for (d, reference) in [(240.0, 96.0), (290.0, 62.0), (340.0, 43.0), (440.0, 24.0), (640.0, 10.0)] {
            let ours = at(&none, d)[0] * 255.0;
            assert!((ours - reference).abs() <= 6.0, "d {d}: ours {ours:.1}, reference {reference}");
        }

        flat[16] = 2.0; // 3-Color Gradient
        let mut grad = disc();
        shine(&mut grad, w, h, &Params::from_flat(&flat), 0.0, 1.0);
        let c = at(&grad, 240.0).map(|v| v * 255.0);
        assert!((c[0] - 96.0).abs() <= 6.0 && (c[1] - 16.0).abs() <= 6.0 && c[2] < 3.0, "3-color at d 240: {c:?}, reference [96, 16, 0]");
    }

    /// Reference renders of a white disc (r 100 px) centred on the source, Colorize None: mean ray
    /// brightness on rings around it at Ray Length 1 and 8, here at quarter size.
    #[test]
    fn matches_reference_ray_lengths() {
        let (w, h) = (480, 270);
        let mut flat = defaults(w as f64, h as f64);
        flat[16] = 0.0; // Colorize None
        for (len, rings) in [(1.0, [(110.0, 0.415), (130.0, 0.089)]), (8.0, [(200.0, 0.435), (420.0, 0.183)])] {
            flat[6] = len;
            let mut px = vec![[0.0f32, 0.0, 0.0, 1.0]; w * h];
            for y in 0..h {
                for x in 0..w {
                    if ((x as f32 + 0.5 - 240.0).powi(2) + (y as f32 + 0.5 - 135.0).powi(2)).sqrt() <= 25.0 {
                        px[y * w + x] = [1.0; 4];
                    }
                }
            }
            shine(&mut px, w, h, &Params::from_flat(&flat), 0.0, 1.0);
            for (r, reference) in rings {
                let r = r / 4.0;
                let n = 360;
                let mean: f32 = (0..n)
                    .map(|i| {
                        let a = i as f32 / n as f32 * TAU;
                        // Screen over black leaves the shine itself.
                        px[(135.0 + r * a.sin()) as usize * w + (240.0 + r * a.cos()) as usize][0]
                    })
                    .sum::<f32>()
                    / n as f32;
                assert!((mean - reference).abs() < 0.04, "Ray Length {len}, r {}: ours {mean:.3}, reference {reference}", r * 4.0);
            }
        }
    }

    /// The fractal noise modulates the light that is there: on black it adds nothing.
    #[test]
    fn fractal_noise_keeps_black_black() {
        let mut flat = defaults(64.0, 64.0);
        flat[39] = 1.0; // Fractal Noise on (Add, Opacity 50)
        let mut px = vec![[0.0f32, 0.0, 0.0, 1.0]; 64 * 64];
        shine(&mut px, 64, 64, &Params::from_flat(&flat), 1.0, 1.0);
        assert!(px.iter().all(|c| c[0] == 0.0 && c[1] == 0.0 && c[2] == 0.0));
    }

    #[test]
    fn shimmer_loop_is_seamless() {
        let mut flat = defaults(64.0, 64.0);
        flat[7] = 100.0; // shimmer amount
        flat[13] = 1.0; // use loop
        flat[14] = 2.0; // 2 revolutions
        let run = |phase: f64| {
            let mut f = flat.clone();
            f[12] = phase;
            let mut px = spot();
            shine(&mut px, 64, 64, &Params::from_flat(&f), 0.0, 1.0);
            px
        };
        let close = |a: Vec<Px>, b: Vec<Px>| a.iter().zip(&b).all(|(x, y)| (0..4).all(|c| (x[c] - y[c]).abs() < 1e-4));
        assert!(close(run(0.0), run(720.0)), "a 2-revolution loop repeats after 720 degrees");
        assert_ne!(run(0.0), run(90.0));
    }

    #[test]
    fn extend_past_layer_pads_by_the_ray_reach_up_to_the_long_side() {
        let mut flat = defaults(200.0, 100.0);
        assert_eq!(padding(&Params::from_flat(&flat), 1.0, [200.0, 100.0]), 0, "off by default");
        flat[59] = 1.0;
        flat[6] = 1.0; // Ray Length 1: far corner ≈ 111.8 px · (255^(1/9.5) − 1) ≈ 88.5 px, rounded up
        assert_eq!(padding(&Params::from_flat(&flat), 1.0, [200.0, 100.0]), 89);
        flat[6] = 4.0;
        assert_eq!(padding(&Params::from_flat(&flat), 1.0, [200.0, 100.0]), 200, "capped at the long side");
        flat[6] = 0.0;
        assert_eq!(padding(&Params::from_flat(&flat), 1.0, [200.0, 100.0]), 0, "no rays, no padding");
    }

    /// The polar blur computes the same rays as the 3-pass blur (which samples them 512 times).
    #[test]
    fn polar_blur_matches_the_tap_blur() {
        let (w, h) = (160, 120);
        // Bright blobs and thin lines on black: what rays are made of.
        let img: Vec<[f32; 2]> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f32, (i / w) as f32);
                let v = if (x - 60.0).hypot(y - 40.0) < 12.0 || (x as usize % 17 == 3 && y > 70.0) { 1.0 } else { 0.0 };
                [v, v]
            })
            .collect();
        for (len, moment) in [(1.0, false), (4.0, true), (10.0, true)] {
            let decay = RAY_DECAY / len;
            let u = -(RAY_CUT / decay).min(7.0);
            let mut a = img.clone();
            radial_blur(&mut a, w, h, 90.0, 70.0, u, decay, moment);
            let mut src = vec![[0.0f32; 1]; (w + 4) * (h + 4)];
            for (i, v) in img.iter().enumerate() {
                src[(i / w + 2) * (w + 4) + i % w + 2] = [v[0]];
            }
            let mut b = vec![[0.0f32; 2]; w * h];
            polar_blur(&src, w, h, 1.0, &mut b, w, 90.0, 70.0, u, decay, moment);
            for c in 0..2 {
                let peak = a.iter().map(|p| p[c]).fold(0.0f32, f32::max);
                let mean = a.iter().zip(&b).map(|(p, q)| (p[c] - q[c]).abs()).sum::<f32>() / (w * h) as f32;
                assert!(mean < 0.005 * peak, "len {len} channel {c}: mean |diff| {mean} vs peak {peak}");
            }
        }
    }
}
