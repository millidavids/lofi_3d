//! Command line. Besides the map, the options are for visual checks and
//! measurement, run by hand or by scripts.

use std::ffi::OsString;
use std::path::PathBuf;

use bevy::prelude::*;

use crate::view::Overlays;

pub const USAGE: &str = "usage: <tool> [MAP.lofimap] [--screenshot OUT.png] \
[--camera EX,EY,EZ,TX,TY,TZ] [--flat] [--overlays slope,grid,full,cells] [--measure]";

#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct LaunchOptions {
    /// Opened on start.
    pub map: Option<PathBuf>,
    /// Save a picture of the settled scene here, then quit.
    pub screenshot: Option<PathBuf>,
    /// Start the camera here, looking at the second point, instead of framing the map.
    pub camera: Option<(Vec3, Vec3)>,
    /// Start in flat shading.
    pub flat: bool,
    pub overlays: Overlays,
    /// Frame the whole map from high up with fog and vsync off, print frame
    /// times, then quit.
    pub measure: bool,
}

impl LaunchOptions {
    /// Parses arguments after the program name.
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--screenshot" {
                let path = args.next().ok_or("--screenshot needs a file path")?;
                options.screenshot = Some(path.into());
            } else if arg == "--camera" {
                let spec = args.next().ok_or("--camera needs six numbers")?;
                options.camera = Some(parse_camera(&spec.to_string_lossy())?);
            } else if arg == "--flat" {
                options.flat = true;
            } else if arg == "--overlays" {
                let list = args.next().ok_or("--overlays needs a list")?;
                options.overlays = parse_overlays(&list.to_string_lossy())?;
            } else if arg == "--measure" {
                options.measure = true;
            } else if arg.to_string_lossy().starts_with("--") {
                return Err(format!("unknown option {}", arg.to_string_lossy()));
            } else if options.map.is_none() {
                options.map = Some(arg.into());
            } else {
                return Err("only one map can be opened".to_owned());
            }
        }
        Ok(options)
    }
}

fn parse_overlays(list: &str) -> Result<Overlays, String> {
    let mut overlays = Overlays::default();
    for name in list.split(',').map(str::trim) {
        match name {
            "slope" => overlays.slope = true,
            "grid" => overlays.chunk_grid = true,
            "full" => overlays.full_chunks = true,
            "cells" => overlays.cell_flags = true,
            other => return Err(format!("--overlays: unknown overlay {other}")),
        }
    }
    Ok(overlays)
}

fn parse_camera(spec: &str) -> Result<(Vec3, Vec3), String> {
    let numbers: Vec<f32> = spec
        .split(',')
        .map(|n| {
            n.trim()
                .parse::<f32>()
                .map_err(|e| format!("--camera: {e}"))
        })
        .collect::<Result<_, _>>()?;
    let [ex, ey, ez, tx, ty, tz] = numbers[..] else {
        return Err("--camera needs six numbers: eye x,y,z then target x,y,z".to_owned());
    };
    let (eye, target) = (Vec3::new(ex, ey, ez), Vec3::new(tx, ty, tz));
    if !eye.is_finite() || !target.is_finite() || eye.distance(target) < 0.01 {
        return Err("--camera eye and target must be finite and apart".to_owned());
    }
    Ok((eye, target))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<LaunchOptions, String> {
        LaunchOptions::parse(args.iter().map(OsString::from))
    }

    #[test]
    fn parses_map_and_screenshot_in_any_order() {
        let expected = LaunchOptions {
            map: Some("zone.lofimap".into()),
            screenshot: Some("shot.png".into()),
            ..LaunchOptions::default()
        };
        assert_eq!(
            parse(&["zone.lofimap", "--screenshot", "shot.png"]),
            Ok(expected.clone())
        );
        assert_eq!(
            parse(&["--screenshot", "shot.png", "zone.lofimap"]),
            Ok(expected)
        );
        assert_eq!(parse(&[]), Ok(LaunchOptions::default()));
    }

    #[test]
    fn parses_camera_and_flags() {
        let options = parse(&["--camera", "1,2,3, 4,5,6", "--flat", "--measure"]).expect("valid");
        assert_eq!(
            options.camera,
            Some((Vec3::new(1.0, 2.0, 3.0), Vec3::new(4.0, 5.0, 6.0)))
        );
        assert!(options.flat && options.measure);
        let options = parse(&["--overlays", "slope, grid"]).expect("valid");
        assert!(options.overlays.slope && options.overlays.chunk_grid);
        assert!(!options.overlays.full_chunks);
        assert!(parse(&["--overlays", "fog"]).is_err());
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(parse(&["--screenshot"]).is_err());
        assert!(parse(&["--fast"]).is_err());
        assert!(parse(&["a.lofimap", "b.lofimap"]).is_err());
        assert!(parse(&["--camera", "1,2,3"]).is_err());
        assert!(
            parse(&["--camera", "1,2,3,1,2,3"]).is_err(),
            "eye on target"
        );
    }
}
