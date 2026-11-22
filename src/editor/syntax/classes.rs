//! What a region is painted as: the `def:` region it is, or the nearest of its parents that is one,
//! among those Colorer's console colours give a colour (`hrd/console/default.hrd`); the theme's roles
//! `syntax.*` colour them (ADR-0024 §3).

use super::hrc::{Library, RegionId};

/// The `def:` regions with a colour of their own in Colorer's console colours.
pub const CLASSES: &[&str] = &[
    "Number",
    "NumberSuffix",
    "String",
    "StringContent",
    "StringEdge",
    "Character",
    "CharacterContent",
    "Comment",
    "CommentContent",
    "CommentEdge",
    "CommentDoc",
    "CommentDocContent",
    "CommentDocEdge",
    "Symbol",
    "SymbolStrong",
    "Prefix",
    "PrefixStrong",
    "Operator",
    "Keyword",
    "TypeKeyword",
    "Register",
    "Constant",
    "BooleanConstant",
    "Var",
    "VarStrong",
    "Identifier",
    "Directive",
    "DirectiveInternal",
    "Parameter",
    "ParameterStrong",
    "ParameterUnknown",
    "Tag",
    "OpenTag",
    "CloseTag",
    "Label",
    "LabelStrong",
    "Insertion",
    "Error",
    "ErrorText",
    "TODO",
    "Debug",
    "URI",
    "EMail",
    "Date",
    "Time",
    "PairStart",
    "PairEnd",
    "PairStrongStart",
    "PairStrongEnd",
];

/// The class of a region (`RegionMapper::getRegionDefine`'s walk up the parents).
pub fn class_of(lib: &Library, region: RegionId) -> Option<&'static str> {
    let mut at = Some(region);
    while let Some(id) = at {
        let name = &lib.region(id).name;
        if let Some(short) = name.strip_prefix("def:")
            && let Some(class) = CLASSES.iter().find(|class| **class == short)
        {
            return Some(class);
        }
        at = lib.region(id).parent;
    }
    None
}
