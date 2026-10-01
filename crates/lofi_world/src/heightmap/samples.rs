//! A heightmap in memory, and fitting it to a map: scale, flip, crop.

/// What the samples mean.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    /// 0..1 across a height range the file doesn't carry.
    Normalized,
    Metres,
}

/// Row-major samples, north row first, like the map's vertices.
#[derive(Clone, Debug, PartialEq)]
pub struct Heightmap {
    pub width: u32,
    pub height: u32,
    pub samples: Vec<f32>,
    pub units: Units,
}

impl Heightmap {
    /// Metres, mapping normalised samples onto `min..=max`. Metres pass
    /// through untouched.
    #[must_use]
    pub fn into_metres(mut self, min: f32, max: f32) -> Self {
        if self.units == Units::Normalized {
            for s in &mut self.samples {
                *s = min + *s * (max - min);
            }
            self.units = Units::Metres;
        }
        self
    }

    pub(super) fn at(&self, x: u32, z: u32) -> f32 {
        self.samples[z as usize * self.width as usize + x as usize]
    }

    /// Stretched to `width`×`height`; corners map to corners. One axis at a
    /// time: linear where it grows or shrinks a little, an area average
    /// where it shrinks by half or more, which linear would alias.
    #[must_use]
    pub fn resample(&self, width: u32, height: u32) -> Self {
        let row = width as usize;
        let mut across = Vec::with_capacity(row * self.height as usize);
        for z in 0..self.height {
            across.extend(resample_line(|x| self.at(x, z), self.width, width));
        }
        let mut samples = vec![0.0; row * height as usize];
        for x in 0..row {
            let column = resample_line(|z| across[z as usize * row + x], self.height, height);
            for (z, v) in column.into_iter().enumerate() {
                samples[z * row + x] = v;
            }
        }
        Self {
            width,
            height,
            samples,
            units: self.units,
        }
    }

    /// Upside down: the last row first. Other tools put row 0 at the far
    /// edge.
    #[must_use]
    pub fn flipped(&self) -> Self {
        let row = self.width as usize;
        let samples = self.samples.chunks(row).rev().flatten().copied().collect();
        Self {
            samples,
            ..self.clone()
        }
    }

    /// Cut or extended to `width`×`height` from the top-left corner; new
    /// samples repeat the nearest edge.
    #[must_use]
    pub fn crop_or_pad(&self, width: u32, height: u32) -> Self {
        let samples = (0..height)
            .flat_map(|z| {
                (0..width).map(move |x| self.at(x.min(self.width - 1), z.min(self.height - 1)))
            })
            .collect();
        Self {
            width,
            height,
            samples,
            units: self.units,
        }
    }
}

/// One line of `from` samples as `to`, first and last kept in place. In
/// f64, so the last sample's position can't round past the end.
fn resample_line(sample: impl Fn(u32) -> f32, from: u32, to: u32) -> Vec<f32> {
    let last = from - 1;
    let step = if to > 1 {
        f64::from(last) / f64::from(to - 1)
    } else {
        0.0
    };
    (0..to)
        .map(|i| {
            let centre = (f64::from(i) * step).clamp(0.0, f64::from(last));
            if step >= 2.0 {
                // The mean within half a step, narrowed at the ends to stay
                // centred, so edges keep their heights.
                let reach = (step * 0.5)
                    .min(centre)
                    .min(f64::from(last) - centre)
                    .max(0.0);
                let lo = (centre - reach).ceil() as u32;
                let hi = ((centre + reach).floor() as u32).min(last).max(lo);
                let sum: f64 = (lo..=hi).map(|j| f64::from(sample(j))).sum();
                (sum / f64::from(hi - lo + 1)) as f32
            } else {
                let i0 = (centre.floor() as u32).min(last);
                let i1 = (i0 + 1).min(last);
                let t = (centre - f64::from(i0)) as f32;
                sample(i0) + (sample(i1) - sample(i0)) * t
            }
        })
        .collect()
}
