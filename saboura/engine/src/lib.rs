//! Saboura's board engine, a port of the JS whiteboard's model: the board file format,
//! object geometry, hit testing and undo. It has no interface code.

pub mod arrange;
pub mod edit;
pub mod export;
pub mod geometry;
pub mod miro;
pub mod model;
pub mod rich;
pub mod sab;

pub use geometry::{Box2, View};
pub use model::{Board, History, Item};
