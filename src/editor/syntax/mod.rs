//! Syntax highlighting as FAR's Colorer (ADR-0024): its regular expressions (`regex`), its HRC grammars
//! and its text parser ported from Colorer-library, used by the editor and the viewer alike.

pub mod classes;
pub mod highlight;
pub mod hrc;
pub mod outline;
pub mod parser;
pub mod regex;
pub mod regions;
pub mod xml;
