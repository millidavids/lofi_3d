//! "Cook for game": writes the cooked world on a worker thread, from a
//! copy-on-write snapshot, so a big map doesn't freeze the editor.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use lofi_world::WorldMap;
use lofi_world::cook::{CookError, CookSource, DEFAULT_TILE_CHUNKS, Manifest, cook_map};

use super::status::StatusLine;

#[derive(Resource, Default)]
pub struct CookJob {
    task: Option<(PathBuf, Task<Result<Manifest, CookError>>)>,
}

impl CookJob {
    pub fn is_running(&self) -> bool {
        self.task.is_some()
    }

    /// Starts cooking `map` into `dir`, unless a cook is already running.
    pub(super) fn start(
        &mut self,
        map: WorldMap,
        source: CookSource,
        dir: PathBuf,
        status: &mut StatusLine,
    ) {
        if self.is_running() {
            status.error("Already cooking; wait for it to finish.");
            return;
        }
        status.info(format!("Cooking into {}…", dir.display()));
        let out = dir.clone();
        let task = AsyncComputeTaskPool::get()
            .spawn(async move { cook_map(&map, &out, DEFAULT_TILE_CHUNKS, source) });
        self.task = Some((dir, task));
    }
}

pub(super) fn poll_cook(mut job: ResMut<CookJob>, mut status: ResMut<StatusLine>) {
    let Some((_, task)) = &mut job.task else {
        return;
    };
    let Some(result) = block_on(poll_once(task)) else {
        return;
    };
    let Some((dir, _)) = job.task.take() else {
        return;
    };
    match result {
        Ok(manifest) => status.info(format!(
            "Cooked into {} ({} tiles)",
            dir.display(),
            manifest.tiles.len()
        )),
        Err(err) => status.error(format!("Could not cook into {}: {err}", dir.display())),
    }
}
