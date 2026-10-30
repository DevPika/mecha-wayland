//! Externals: textures the atlas names but does not hold, described by
//! the dmabuf a producer hands over and imported by a backend with no
//! copy.

use std::os::fd::OwnedFd;
use std::sync::Arc;

use geometry::{Rect, Size};

use crate::{Atlas, AtlasId, AtlasTile, Owner};

/// One plane of a dmabuf. The `Arc` keeps the buffer open while any
/// descriptor names it, so a producer hands it over and moves on.
#[derive(Debug, Clone)]
pub struct Plane {
    pub fd: Arc<OwnedFd>,
    pub offset: u32,
    pub stride: u32,
}

/// A texture produced outside the atlas: a camera feed, a screen
/// capture, another client's surface. A backend reads `fourcc` to pick
/// its import path and re-imports when `generation` moves. The producer
/// bumps `generation` when `planes` holds a new buffer and raises
/// `RequestFrame` itself.
#[derive(Debug, Clone)]
pub struct External {
    /// In pixels.
    pub size: Size,
    /// DRM format code.
    pub fourcc: u32,
    /// DRM format modifier.
    pub modifier: u64,
    /// One to four.
    pub planes: Vec<Plane>,
    pub generation: u64,
}

impl Atlas {
    /// Name a new external. Its `AtlasId` shares the id space with pages.
    pub fn external(&mut self, desc: External) -> AtlasId {
        let index = self.externals.len() as u32;
        let id = self.mint(Owner::External(index));
        self.externals.push(desc);
        id
    }

    /// The descriptor, to write new planes into and bump. Panics if `id`
    /// is not an external of this atlas.
    pub fn external_mut(&mut self, id: AtlasId) -> &mut External {
        match self.owners[id.0 as usize] {
            Owner::External(i) => &mut self.externals[i as usize],
            Owner::Page(..) | Owner::Free => panic!("{id:?} is not an external"),
        }
    }

    /// A tile over the whole texture, for a `PolychromeSprite` that shows
    /// the feed. Panics if `id` is not an external of this atlas.
    pub fn external_tile(&self, id: AtlasId) -> AtlasTile {
        let e = match self.owners[id.0 as usize] {
            Owner::External(i) => &self.externals[i as usize],
            Owner::Page(..) | Owner::Free => panic!("{id:?} is not an external"),
        };
        AtlasTile {
            atlas: id,
            bounds: Rect::new(0.0, 0.0, e.size.width, e.size.height),
        }
    }

    /// Every external with its id, in id order.
    pub fn externals(&self) -> impl Iterator<Item = (AtlasId, &External)> {
        self.owners
            .iter()
            .enumerate()
            .filter_map(|(n, o)| match *o {
                Owner::External(i) => Some((AtlasId(n as u32), &self.externals[i as usize])),
                Owner::Page(..) | Owner::Free => None,
            })
    }
}
