//! HRC files read (`HrcLibraryImpl::loadSource` and its `parse...` / `add...`): prototypes and packages
//! when the catalog is read, a type's regions, entities, imports and schemes when it is first wanted.
//!
//! Ported from Colorer-library (MIT; Copyright (c) 2009 Igor Russkih, 2009 and present Aleksey Dobrunov).

use std::path::Path;

use super::super::regex::Regex;
use super::super::xml::{self, Element};
use super::{
    BlockNode, Chooser, ChooserKind, FileType, InheritNode, KeywordList, KeywordsNode, Library,
    LoadType, Node, Param, ReNode, Scheme, TypeId, Virtual,
};

impl Library {
    /// A file read as `load_type` (`loadSource`).
    pub(super) fn load_source(&mut self, path: &Path, load_type: LoadType) {
        let saved = (std::mem::take(&mut self.current_file), self.load_type);
        self.current_file = path.to_path_buf();
        self.load_type = load_type;
        let read = xml::read(path);
        let result = read.and_then(|root| self.parse_hrc(&root));
        if let Err(error) = result {
            self.log.push(error);
            if let Some(id) = self.loading_type(path) {
                self.types[id].load_broken = true;
            }
        }
        (self.current_file, self.load_type) = saved;
    }

    fn loading_type(&self, path: &Path) -> Option<TypeId> {
        self.types
            .iter()
            .position(|t| t.source_loading && t.source == path)
    }

    /// A type's file read for its schemes (`loadFileType`); none again once it is read or broken.
    pub(super) fn load_file_type(&mut self, id: TypeId) {
        let file_type = &self.types[id];
        if file_type.type_loading || file_type.source_loading || file_type.load_broken {
            return;
        }
        self.types[id].source_loading = true;
        let source = self.types[id].source.clone();
        self.load_source(&source, LoadType::Type);
        let file_type = &mut self.types[id];
        file_type.source_loading = false;
        if file_type.load_broken {
            self.type_content_loaded = true;
        }
    }

    /// `parseHRC`: the `<hrc>` block, the links put right after the outermost one.
    fn parse_hrc(&mut self, root: &Element) -> Result<(), String> {
        if root.name != "hrc" {
            return Err(format!(
                "Incorrect hrc-file structure. Main '<hrc>' block not found. Current file {}",
                self.current_file.display()
            ));
        }
        let outermost = !self.update_started;
        self.update_started = true;
        for node in &root.children {
            match node.name.as_str() {
                "prototype" | "package" => self.add_prototype(node),
                "type" => self.add_type(node),
                _ => {}
            }
        }
        self.structure_changed = true;
        if outermost {
            self.update_links();
            self.update_started = false;
        }
        Ok(())
    }

    /// `<prototype>` / `<package>` (`addPrototype`).
    fn add_prototype(&mut self, elem: &Element) {
        let global = matches!(elem.value("global"), "" | "yes");
        let package = elem.name == "package";
        let skip = match self.load_type {
            LoadType::Type => !package || global,
            LoadType::Prototype => package && !global,
        };
        let name = elem.value("name").to_string();
        if skip || name.is_empty() {
            return;
        }
        if let Some(&old) = self.type_names.get(&name) {
            self.listed.retain(|&id| id != old);
            self.type_names.remove(&name);
            self.log.push(format!("Duplicate prototype '{name}'."));
        }
        let (group, description) = (elem.value("group"), elem.value("description"));
        let mut file_type = FileType {
            group: if group.is_empty() {
                name.clone()
            } else {
                group.to_string()
            },
            description: if description.is_empty() {
                name.clone()
            } else {
                description.to_string()
            },
            name: name.clone(),
            is_package: package,
            ..FileType::default()
        };
        self.parse_prototype_block(elem, &mut file_type);
        let id = self.types.len();
        self.types.push(file_type);
        self.type_names.insert(name, id);
        if !package {
            self.listed.push(id);
        }
    }

    /// `parsePrototypeBlock`: its location (the current file without one), choosers, parameters.
    fn parse_prototype_block(&mut self, elem: &Element, file_type: &mut FileType) {
        for node in &elem.children {
            match node.name.as_str() {
                "location" => {
                    let link = node.value("link");
                    if link.is_empty() {
                        self.log
                            .push(format!("Bad 'location' in prototype '{}'", file_type.name));
                    } else {
                        file_type.source = self.relative(link);
                    }
                }
                "filename" | "firstline" if !file_type.is_package => {
                    self.add_detect_param(node, file_type);
                }
                "parameters" if !file_type.is_package => add_parameters(node, file_type),
                _ => {}
            }
        }
        if file_type.source.as_os_str().is_empty() {
            file_type.source = self.current_file.clone();
        }
    }

    fn relative(&self, link: &str) -> std::path::PathBuf {
        self.current_file
            .parent()
            .unwrap_or(Path::new("."))
            .join(link)
    }

    /// `<filename>` / `<firstline>` (`addPrototypeDetectParam`): weight 2 and 1 by default.
    fn add_detect_param(&mut self, elem: &Element, file_type: &mut FileType) {
        if elem.text.is_empty() {
            return;
        }
        let Ok(mut regex) = Regex::new(&elem.text) else {
            self.log
                .push(format!("Fault compiling chooser RE '{}'", elem.text));
            return;
        };
        regex.set_position_moves(true);
        let kind = if elem.name == "filename" {
            ChooserKind::FileName
        } else {
            ChooserKind::FirstLine
        };
        let mut priority = if kind == ChooserKind::FileName {
            2.0
        } else {
            1.0
        };
        if let Ok(weight) = elem.value("weight").trim().parse::<f64>()
            && weight >= 0.0
        {
            priority = weight;
        }
        file_type.choosers.push(Chooser {
            kind,
            priority,
            regex,
        });
    }

    /// `<type>` (`addType`): its block read, its base scheme the one named as it.
    fn add_type(&mut self, elem: &Element) {
        if self.load_type == LoadType::Prototype {
            return;
        }
        let name = elem.value("name");
        let Some(&id) = self.type_names.get(name) else {
            self.log.push(format!("Type '{name}' without prototype"));
            return;
        };
        if self.types[id].type_loading {
            return;
        }
        self.types[id].type_loading = true;
        self.type_content_loaded = true;
        let outer = self.current_type.replace(id);
        for node in &elem.children {
            match node.name.as_str() {
                "region" => self.add_region(node),
                "entity" => self.add_entity(node),
                "import" => self.add_import(node),
                "scheme" => self.add_scheme(node),
                _ => {}
            }
        }
        let base = self.qualify_own_name(&self.types[id].name.clone());
        self.types[id].base_scheme = base.and_then(|base| self.scheme_names.get(&base).copied());
        self.types[id].load_done = true;
        self.current_type = outer;
    }

    /// `<region name= parent= description=>` (`addTypeRegion`).
    fn add_region(&mut self, elem: &Element) {
        let name = elem.value("name");
        if name.is_empty() {
            return;
        }
        let Some(qualified) = self.qualify_own_name(name) else {
            return;
        };
        if self.region_names.contains_key(&qualified) {
            self.log.push(format!("Duplicate region '{qualified}'"));
            return;
        }
        let parent = elem.value("parent");
        let parent = (!parent.is_empty())
            .then(|| self.qualify_foreign_name(parent, super::names::Kind::Region, true))
            .flatten();
        let parent = parent.and_then(|name| self.nc_region(&name, false));
        let id = self.regions.len();
        self.regions.push(super::Region {
            name: qualified.clone(),
            #[cfg(test)]
            description: elem.value("description").to_string(),
            parent,
        });
        self.region_names.insert(qualified, id);
    }

    /// `<entity name= value=>` (`addTypeEntity`): its value with its own entities put in.
    fn add_entity(&mut self, elem: &Element) {
        let name = elem.value("name");
        let Some(value) = elem.attr("value").filter(|_| !name.is_empty()) else {
            return;
        };
        let value = self.use_entities(value);
        if let Some(qualified) = self.qualify_own_name(name) {
            self.entities.entry(qualified).or_insert(value);
        }
    }

    /// `<import type=>` (`addTypeImport`).
    fn add_import(&mut self, elem: &Element) {
        let name = elem.value("type");
        if name.is_empty() || !self.type_names.contains_key(name) {
            self.log.push(format!("Import with bad type '{name}'"));
            return;
        }
        if let Some(id) = self.current_type {
            self.types[id].imports.push(name.to_string());
        }
    }

    /// `<scheme name= if= unless=>` (`addScheme`): kept empty when its condition is off.
    fn add_scheme(&mut self, elem: &Element) {
        let Some(name) = self.qualify_own_name(elem.value("name")) else {
            return;
        };
        if self.scheme_names.contains_key(&name) {
            self.log.push(format!("duplicate scheme name '{name}'"));
            return;
        }
        let Some(type_id) = self.current_type else {
            return;
        };
        let id = self.schemes.len();
        self.schemes.push(Scheme {
            name: name.clone(),
            file_type: type_id,
            nodes: Vec::new(),
            search: Vec::new(),
            owned: Vec::new(),
            dispatch: None,
            virtual_target: false,
        });
        self.scheme_names.insert(name, id);
        let file_type = &self.types[type_id];
        let (cond_if, cond_unless) = (elem.value("if"), elem.value("unless"));
        let off = !cond_if.is_empty() && file_type.param(cond_if).is_some_and(|v| v != "true")
            || !cond_unless.is_empty() && file_type.param(cond_unless) == Some("true");
        if off {
            return;
        }
        for node in &elem.children {
            let node = match node.name.as_str() {
                "inherit" => self.inherit(node),
                "regexp" => self.regexp(node),
                "block" => self.block(node),
                "keywords" => self.keywords(node),
                _ => None,
            };
            if let Some(node) = node {
                self.nodes.push(node);
                let at = self.nodes.len() - 1;
                self.schemes[id].nodes.push(at);
            }
        }
    }

    /// `<inherit scheme=>` with its `<virtual>`s (`addSchemeInherit`).
    fn inherit(&mut self, elem: &Element) -> Option<Node> {
        let name = elem.value("scheme");
        if name.is_empty() {
            return None;
        }
        let mut node = InheritNode {
            scheme_name: Some(name.to_string()),
            ..InheritNode::default()
        };
        if let Some(qualified) = self.qualify_foreign_name(name, super::names::Kind::Scheme, false)
        {
            node.scheme = self.scheme_names.get(&qualified).copied();
            node.scheme_name = Some(qualified);
        }
        for child in elem.children.iter().filter(|child| child.name == "virtual") {
            let (virt, subst) = (child.value("scheme"), child.value("subst-scheme"));
            if virt.is_empty() || subst.is_empty() {
                continue;
            }
            node.virtuals.push(Virtual {
                virt_name: Some(virt.to_string()),
                subst_name: Some(subst.to_string()),
                ..Virtual::default()
            });
        }
        Some(Node::Inherit(node))
    }

    /// `<regexp match= region=...>` (`addSchemeRegexp`).
    fn regexp(&mut self, elem: &Element) -> Option<Node> {
        let mut pattern = elem.value("match");
        if pattern.is_empty() {
            pattern = &elem.text;
        }
        if pattern.is_empty() {
            return None;
        }
        let pattern = self.use_entities(pattern);
        let Ok(start) = Regex::new(&pattern) else {
            self.log.push(format!("fault compiling regexp '{pattern}'"));
            return None;
        };
        let mut node = ReNode {
            low_priority: elem.value("priority") == "low",
            start,
            region: None,
            regions: Default::default(),
            regions_n: Default::default(),
        };
        self.regexp_regions(&mut node, elem);
        if node.region.is_some() {
            node.regions[0] = node.region;
        }
        Some(Node::Re(Box::new(node)))
    }

    /// `<block start= end= scheme=...>` or with `<start>` / `<end>` elements (`addSchemeBlock`).
    fn block(&mut self, elem: &Element) -> Option<Node> {
        let (mut start, mut end) = (elem.value("start"), elem.value("end"));
        let (mut start_elem, mut end_elem) = (None, None);
        if start.is_empty() && end.is_empty() {
            for child in &elem.children {
                let text = child.attr("match").unwrap_or(&child.text);
                match child.name.as_str() {
                    "start" => (start_elem, start) = (Some(child), text),
                    "end" => (end_elem, end) = (Some(child), text),
                    _ => {}
                }
            }
        }
        let scheme = elem.value("scheme");
        if start.is_empty() || end.is_empty() || scheme.is_empty() {
            self.log
                .push("a block without its start, end or scheme".to_string());
            return None;
        }
        let start_pattern = self.use_entities(start);
        let end_pattern = self.use_entities(end);
        let Ok(start) = Regex::new(&start_pattern) else {
            self.log
                .push(format!("fault compiling start regexp '{start_pattern}'"));
            return None;
        };
        let Ok(mut end) = Regex::with_back(&end_pattern, Some(&start)) else {
            self.log
                .push(format!("fault compiling end regexp '{end_pattern}'"));
            return None;
        };
        end.set_position_moves(true);
        let mut node = BlockNode {
            inner_region: elem.value("inner-region") == "yes",
            low_priority: elem.value("priority") == "low",
            low_content_priority: elem.value("content-priority") == "low",
            scheme_name: Some(scheme.to_string()),
            scheme: None,
            start,
            end,
            region: None,
            regions: Default::default(),
            regions_n: Default::default(),
            regione: Default::default(),
            regionen: Default::default(),
        };
        self.block_regions(&mut node, elem);
        self.element_regions(&mut node, start_elem, true);
        self.element_regions(&mut node, end_elem, false);
        Some(Node::Block(Box::new(node)))
    }

    /// `<keywords region= ignorecase= worddiv=>` with its `<word>`s and `<symb>`s.
    fn keywords(&mut self, elem: &Element) -> Option<Node> {
        let Some(region) = self.nc_region_attr(elem, "region") else {
            self.log
                .push("a keywords block without its region".to_string());
            return None;
        };
        let worddiv = elem.value("worddiv");
        let worddiv = (!worddiv.is_empty())
            .then(|| {
                let chars: Vec<char> = self.use_entities(worddiv).chars().collect();
                super::super::regex::char_class(&chars, 0, false).map(|(class, _)| class)
            })
            .flatten();
        let match_case = elem.value("ignorecase") != "yes";
        let mut list = KeywordList::new(match_case);
        for child in &elem.children {
            let symbol = match child.name.as_str() {
                "word" => false,
                "symb" => true,
                _ => continue,
            };
            let name = child.value("name");
            if name.is_empty() {
                continue;
            }
            let own = if child.value("region").is_empty() {
                Some(region)
            } else {
                self.nc_region_attr(child, "region")
            };
            let word: Vec<char> = if match_case {
                name.chars().collect()
            } else {
                // One character for one, as the parser lowers the line (review B10: `İ` became two
                // and the word never matched).
                name.chars().map(super::super::regex::to_lower).collect()
            };
            list.add(word, own, symbol);
        }
        list.sort();
        list.index();
        Some(Node::Keywords(Box::new(KeywordsNode { list, worddiv })))
    }
}

/// `<parameters>` (`addPrototypeParameters`).
fn add_parameters(elem: &Element, file_type: &mut FileType) {
    for node in elem.children.iter().filter(|node| node.name == "param") {
        let (name, value) = (node.value("name"), node.value("value"));
        if name.is_empty() || value.is_empty() {
            continue;
        }
        if file_type.params.iter().any(|(key, _)| key == name) {
            continue;
        }
        file_type.params.push((
            name.to_string(),
            Param {
                value: value.to_string(),
                #[cfg(test)]
                description: node.attr("description").map(str::to_string),
                user: None,
            },
        ));
    }
}
