//! Names in a type (`qualifyOwnName`, `qualifyForeignName`, `useEntities`, `getNCRegion` and the node's
//! regions): `type:name` or a name of the type itself or of a type it imports, a type named loaded on
//! the way; `%entity;` in a pattern put in; a region named `...:default` is none (transparent).
//!
//! Ported from Colorer-library (MIT; Copyright (c) 2009 Igor Russkih, 2009 and present Aleksey Dobrunov).

use super::super::regex::{MATCHES, NAMED, Regex};
use super::super::xml::Element;
use super::{BlockNode, Library, ReNode, RegionId, Regions, TypeId};

/// What a name stands for (`QualifyNameType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Region,
    Entity,
    Scheme,
}

/// `region0`..`region9`, `regiona`..`regionf`: the digit of bracket `i`.
fn digit(i: usize) -> char {
    char::from_digit(i as u32, 16).unwrap_or('0')
}

impl Library {
    /// A name of the type being read made full (`qualifyOwnName`); none when it names another type.
    pub(super) fn qualify_own_name(&mut self, name: &str) -> Option<String> {
        let current = self.current_type.map(|id| self.types[id].name.clone())?;
        if let Some((prefix, _)) = name.split_once(':') {
            if prefix != current {
                self.log.push(format!(
                    "type name qualifer in '{name}' doesn't match current type '{current}'"
                ));
                return None;
            }
            return Some(name.to_string());
        }
        Some(format!("{current}:{name}"))
    }

    fn name_exists(&self, name: &str, kind: Kind) -> bool {
        match kind {
            Kind::Region => self.region_names.contains_key(name),
            Kind::Entity => self.entities.contains_key(name),
            Kind::Scheme => self.scheme_names.contains_key(name),
        }
    }

    /// A name made full (`qualifyForeignName`): `type:name` (the type loaded if it is not), or the
    /// first of the current type and its imports that has it.
    pub(super) fn qualify_foreign_name(
        &mut self,
        name: &str,
        kind: Kind,
        log: bool,
    ) -> Option<String> {
        if let Some((prefix, _)) = name.split_once(':') {
            let Some(&id) = self.type_names.get(prefix) else {
                if log {
                    self.log.push(format!(
                        "type name qualifer in '{name}' doesn't match any type"
                    ));
                }
                return None;
            };
            if !self.types[id].type_loading {
                self.load_file_type(id);
            }
            if Some(id) == self.current_type || self.types[id].type_loading {
                return self.checked(name.to_string(), id, kind, log);
            }
            return None;
        }
        let current = self.current_type?;
        let candidates: Vec<String> = std::iter::once(self.types[current].name.clone())
            .chain(self.types[current].imports.iter().cloned())
            .collect();
        for type_name in candidates {
            let Some(&importer) = self.type_names.get(&type_name) else {
                continue;
            };
            if !self.types[importer].type_loading {
                self.load_file_type(importer);
            }
            let qualified = format!("{type_name}:{name}");
            if self.name_exists(&qualified, kind) {
                return Some(qualified);
            }
        }
        if log {
            self.log.push(format!(
                "unqualified name '{name}' doesn't belong to any imported type [{}]",
                self.current_file.display()
            ));
        }
        None
    }

    fn checked(&mut self, name: String, id: TypeId, kind: Kind, log: bool) -> Option<String> {
        if self.name_exists(&name, kind) {
            return Some(name);
        }
        if log {
            let type_name = &self.types[id].name;
            self.log.push(format!(
                "'{name}', referenced in type '{type_name}', is not defined"
            ));
        }
        None
    }

    /// `%name;` in a pattern put in by the entity it names (`useEntities`); `\%` and unknown ones kept.
    pub(super) fn use_entities(&mut self, text: &str) -> String {
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let (mut copied, mut at) = (0, 0);
        while let Some(found) = chars[at..].iter().position(|&c| c == '%').map(|p| p + at) {
            if found > 0 && chars[found - 1] == '\\' {
                at = found + 1;
                continue;
            }
            let Some(end) = chars[found..]
                .iter()
                .position(|&c| c == ';')
                .map(|p| p + found)
            else {
                break;
            };
            let name: String = chars[found + 1..end].iter().collect();
            let value = self
                .qualify_foreign_name(&name, Kind::Entity, true)
                .and_then(|qualified| self.entities.get(&qualified).cloned());
            let Some(value) = value else {
                at = found + 1;
                continue;
            };
            out.extend(&chars[copied..found]);
            out.push_str(&value);
            at = end + 1;
            copied = at;
        }
        out.extend(&chars[copied..]);
        out
    }

    /// A region by its name (`getNCRegion`): none for a `...:default` one.
    pub(super) fn nc_region(&mut self, name: &str, log: bool) -> Option<RegionId> {
        if name.is_empty() {
            return None;
        }
        let qualified = self.qualify_foreign_name(name, Kind::Region, log)?;
        let id = self.region_names.get(&qualified).copied()?;
        (!self.regions[id].name.ends_with(":default")).then_some(id)
    }

    /// The region an attribute names, when it is there.
    pub(super) fn nc_region_attr(&mut self, elem: &Element, tag: &str) -> Option<RegionId> {
        let name = elem.attr(tag)?.to_string();
        self.nc_region(&name, true)
    }

    fn named_regions(&mut self, regex: &Regex) -> [Option<RegionId>; NAMED] {
        let mut regions = [None; NAMED];
        for (i, region) in regions.iter_mut().enumerate() {
            if let Some(name) = regex.bracket_name(i) {
                *region = self.nc_region(&name, false);
            }
        }
        regions
    }

    fn numbered_regions(&mut self, elem: &Element, prefix: &str) -> Regions {
        let mut regions = [None; MATCHES];
        for (i, region) in regions.iter_mut().enumerate() {
            *region = self.nc_region_attr(elem, &format!("{prefix}{}", digit(i)));
        }
        regions
    }

    /// `loadRegexpRegions`.
    pub(super) fn regexp_regions(&mut self, node: &mut ReNode, elem: &Element) {
        if node.region.is_none() {
            node.region = self.nc_region_attr(elem, "region");
        }
        node.regions = self.numbered_regions(elem, "region");
        node.regions_n = self.named_regions(&node.start);
    }

    /// `loadBlockRegions`: `region`, `region00`.. and `region10`.. of the block's element.
    pub(super) fn block_regions(&mut self, node: &mut BlockNode, elem: &Element) {
        node.region = self.nc_region_attr(elem, "region");
        node.regions = self.numbered_regions(elem, "region0");
        node.regione = self.numbered_regions(elem, "region1");
    }

    /// `loadRegions`: the `<start>` or `<end>` element's regions, the named brackets' always.
    pub(super) fn element_regions(
        &mut self,
        node: &mut BlockNode,
        elem: Option<&Element>,
        start: bool,
    ) {
        if let Some(elem) = elem {
            if node.region.is_none() {
                node.region = self.nc_region_attr(elem, "region");
            }
            let regions = self.numbered_regions(elem, "region");
            if start {
                node.regions = regions;
            } else {
                node.regione = regions;
            }
        }
        if start {
            node.regions_n = self.named_regions(&node.start);
        } else {
            node.regionen = self.named_regions(&node.end);
        }
    }
}
