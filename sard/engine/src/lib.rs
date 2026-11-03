//! The slides engine: PowerPoint (.pptx) presentations as a lossless package model, with
//! theme colors, placeholder inheritance, preset geometry, rendering to a scene and editing.
//! A port of the JS presentation editor's lib/pptx and lib/pptx-preview.

pub mod animation;
pub mod color;
pub mod design;
pub mod diagram;
pub mod edit;
pub mod embedded_fonts;
pub mod export;
pub mod format;
pub mod geom;
pub mod objects;
pub mod mermaid;
pub mod opc;
pub mod pres;
pub mod render;
pub mod scene;
pub mod show_setup;
pub mod table;
pub mod text;
pub mod three_d;
pub mod transition;
pub mod xml;

pub use color::Rgba;
pub use pres::Presentation;
