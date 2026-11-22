//! Colorer's HRC library (`HrcLibraryImpl`, `FileTypeImpl`, `SchemeImpl`, `SchemeNode`, `KeywordList`,
//! `Region`): the catalog's prototypes read at once, a type's schemes when it is first wanted (and the
//! types its names lead to), each scheme's nodes - regular expressions, blocks from a start to an end
//! with a scheme inside, keyword lists, inherited schemes with their virtual substitutions - and the
//! regions they colour, each with its parent.
//!
//! Ported from Colorer-library (MIT; Copyright (c) 2009 Igor Russkih, 2009 and present Aleksey Dobrunov).

mod keywords;
mod links;
mod load;
mod names;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub use keywords::KeywordList;

use super::regex::{CharClass, MATCHES, Matches, NAMED, Parse, Regex};

pub type TypeId = usize;
pub type SchemeId = usize;
pub type RegionId = usize;
pub type NodeId = usize;

/// A region (`Region`): `def:Comment`, its description, its parent.
#[derive(Clone, Debug)]
pub struct Region {
    pub name: String,
    #[cfg(test)]
    pub description: String,
    pub parent: Option<RegionId>,
}

/// A type's parameter (`TypeParameter`): its value from the HRC, the user's over it.
#[derive(Clone, Debug, Default)]
pub struct Param {
    pub value: String,
    #[cfg(test)]
    pub description: Option<String>,
    pub user: Option<String>,
}

/// What a chooser looks at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChooserKind {
    FileName,
    FirstLine,
}

/// A prototype's `<filename>` or `<firstline>` (`FileTypeChooser`).
#[derive(Clone, Debug)]
pub struct Chooser {
    pub kind: ChooserKind,
    pub priority: f64,
    pub regex: Regex,
}

/// A file type (`FileType`): its prototype and, once loaded, its base scheme.
#[derive(Clone, Debug, Default)]
pub struct FileType {
    pub name: String,
    /// For FarColorer's type menu (with the editor's F11 menu, S2d.7).
    pub group: String,
    pub description: String,
    pub is_package: bool,
    source: PathBuf,
    pub choosers: Vec<Chooser>,
    pub params: Vec<(String, Param)>,
    imports: Vec<String>,
    pub base_scheme: Option<SchemeId>,
    type_loading: bool,
    source_loading: bool,
    load_done: bool,
    load_broken: bool,
}

impl FileType {
    /// A parameter's value, the user's first (`getParamValue`).
    pub fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, param)| param.user.as_deref().unwrap_or(&param.value))
    }

    /// A parameter as a number, `default` when it is not one (`getParamValueInt`).
    pub fn param_int(&self, name: &str, default: i32) -> i32 {
        self.param(name)
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(default)
    }

    /// How well a file's name and first line fit it (`getPriority`).
    pub fn priority(&self, file_name: &[char], first_line: &[char]) -> f64 {
        self.choosers
            .iter()
            .map(|chooser| {
                let text = match chooser.kind {
                    ChooserKind::FileName => file_name,
                    ChooserKind::FirstLine => first_line,
                };
                let how = Parse {
                    moves: true,
                    ..Parse::default()
                };
                let matched = chooser.regex.parse(text, &mut Matches::default(), how);
                if matched { chooser.priority } else { 0.0 }
            })
            .sum()
    }
}

/// Regions by bracket number and by bracket name.
pub type Regions = [Option<RegionId>; MATCHES];
pub type NamedRegions = [Option<RegionId>; NAMED];

/// `<regexp>` (`SchemeNodeRegexp`).
#[derive(Clone, Debug)]
pub struct ReNode {
    pub low_priority: bool,
    pub start: Regex,
    pub region: Option<RegionId>,
    pub regions: Regions,
    pub regions_n: NamedRegions,
}

/// `<block>` (`SchemeNodeBlock`).
#[derive(Clone, Debug)]
pub struct BlockNode {
    pub inner_region: bool,
    pub low_priority: bool,
    pub low_content_priority: bool,
    scheme_name: Option<String>,
    pub scheme: Option<SchemeId>,
    pub start: Regex,
    pub end: Regex,
    pub region: Option<RegionId>,
    pub regions: Regions,
    pub regions_n: NamedRegions,
    pub regione: Regions,
    pub regionen: NamedRegions,
}

/// `<keywords>` (`SchemeNodeKeywords`).
#[derive(Clone, Debug)]
pub struct KeywordsNode {
    pub list: KeywordList,
    pub worddiv: Option<CharClass>,
}

/// One `<virtual scheme= subst-scheme=>` of an inherit (`VirtualEntry`).
#[derive(Clone, Debug, Default)]
pub struct Virtual {
    virt_name: Option<String>,
    subst_name: Option<String>,
    pub virt: Option<SchemeId>,
    pub subst: Option<SchemeId>,
}

/// `<inherit>` (`SchemeNodeInherit`).
#[derive(Clone, Debug, Default)]
pub struct InheritNode {
    scheme_name: Option<String>,
    pub scheme: Option<SchemeId>,
    pub virtuals: Vec<Virtual>,
}

/// A scheme's node (`SchemeNode`).
#[derive(Clone, Debug)]
pub enum Node {
    Re(Box<ReNode>),
    Block(Box<BlockNode>),
    Keywords(Box<KeywordsNode>),
    Inherit(InheritNode),
}

/// A node a scheme looks through: its own or another scheme's (inherited), or a merged keyword list it
/// owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeRef {
    Node(NodeId),
    Owned(SchemeId, usize),
}

/// The nodes that may start with each ASCII character (`SearchDispatch`).
#[derive(Clone, Debug)]
pub struct Dispatch {
    pub offsets: [u32; 129],
    pub nodes: Vec<u16>,
}

/// A scheme (`SchemeImpl`).
#[derive(Clone, Debug)]
pub struct Scheme {
    pub name: String,
    pub file_type: TypeId,
    pub nodes: Vec<NodeId>,
    /// The nodes looked through: inherits without virtual substitutions put in place, keyword lists
    /// next to each other merged.
    pub search: Vec<NodeRef>,
    pub owned: Vec<Node>,
    pub dispatch: Option<Dispatch>,
    pub virtual_target: bool,
}

/// What a file is read as (`LoadType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoadType {
    Prototype,
    Type,
}

/// The library (`HrcLibrary`).
#[derive(Debug)]
pub struct Library {
    types: Vec<FileType>,
    type_names: HashMap<String, TypeId>,
    /// The types a file may be chosen as, in their order (`fileTypeVector`).
    listed: Vec<TypeId>,
    regions: Vec<Region>,
    region_names: HashMap<String, RegionId>,
    schemes: Vec<Scheme>,
    scheme_names: HashMap<String, SchemeId>,
    entities: HashMap<String, String>,
    nodes: Vec<Node>,
    current_type: Option<TypeId>,
    current_file: PathBuf,
    load_type: LoadType,
    update_started: bool,
    structure_changed: bool,
    type_content_loaded: bool,
    /// The schemes whose search lists and dispatch were built by the last link update: a later load
    /// builds only its own and those it changes (ours; Colorer rebuilds all, the load's lag).
    linked: usize,
    /// What went wrong, as Colorer logs it.
    pub log: Vec<String>,
}

impl Default for Library {
    fn default() -> Self {
        Library {
            types: Vec::new(),
            type_names: HashMap::new(),
            listed: Vec::new(),
            regions: Vec::new(),
            region_names: HashMap::new(),
            schemes: Vec::new(),
            scheme_names: HashMap::new(),
            entities: HashMap::new(),
            nodes: Vec::new(),
            current_type: None,
            current_file: PathBuf::new(),
            load_type: LoadType::Prototype,
            update_started: false,
            structure_changed: false,
            type_content_loaded: false,
            linked: 0,
            log: Vec::new(),
        }
    }
}

impl Library {
    /// The library of a catalog (`catalog.xml`): the prototypes of its `hrc-sets` read.
    pub fn from_catalog(catalog: &Path) -> Library {
        let mut library = Library::default();
        let root = match super::xml::read(catalog) {
            Ok(root) => root,
            Err(error) => {
                library.log.push(error);
                return library;
            }
        };
        let base = catalog.parent().unwrap_or(Path::new("."));
        let locations = root
            .children
            .iter()
            .filter(|set| set.name == "hrc-sets")
            .flat_map(|set| set.children.iter())
            .filter(|location| location.name == "location")
            .map(|location| location.value("link").to_string())
            .filter(|link| !link.is_empty())
            .collect::<Vec<_>>();
        for link in locations {
            library.load_path(&base.join(link));
        }
        library
    }

    /// A file or a folder's `.hrc` files (not `.ent.hrc`) read for their prototypes (`loadHrcPath`).
    pub fn load_path(&mut self, path: &Path) {
        if path.is_file() {
            self.load_source(path, LoadType::Prototype);
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            self.log.push(format!("cannot read {}", path.display()));
            return;
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|file| {
                let name = file.to_string_lossy();
                name.ends_with(".hrc") && !name.ends_with(".ent.hrc")
            })
            .collect();
        files.sort();
        for file in files {
            self.load_source(&file, LoadType::Prototype);
        }
    }

    /// The type a file is best taken as (`chooseFileType`; `nth` the next ones as good).
    pub fn choose_type(&self, file_name: &str, first_line: &str, mut nth: usize) -> Option<TypeId> {
        let name: Vec<char> = file_name.chars().collect();
        let line: Vec<char> = first_line.chars().collect();
        let mut best = None;
        let mut max = 0.0;
        for &id in &self.listed {
            let priority = self.types[id].priority(&name, &line);
            if nth > 0 && priority - max < 1e-6 {
                best = Some(id);
                nth -= 1;
            }
            if priority - max > 1e-6 || best.is_none() {
                best = Some(id);
                max = priority;
            }
        }
        if nth > 0 { None } else { best }
    }

    /// Colorer's `chooseFileType` with `chooseFileTypeCh` (review B5): the type by the file's name and
    /// its text's start - its first `firstlines` lines, `firstlinebytes` characters of them at most,
    /// the "default" type's or 4 and 800 - chosen again with the chosen type's own when they differ.
    /// `line(i)` gives the text's line `i` (none past its end); `Err` when one is not read yet.
    pub fn choose_by_start(
        &self,
        file_name: &str,
        line: &mut dyn FnMut(usize) -> Result<Option<String>, ()>,
    ) -> Result<Option<TypeId>, ()> {
        let of = |id: Option<TypeId>, (lines, chars): (i32, i32)| match id {
            Some(id) => (
                self.types[id].param_int("firstlines", lines),
                self.types[id].param_int("firstlinebytes", chars),
            ),
            None => (lines, chars),
        };
        let mut wanted = of(self.type_named("default"), (4, 800));
        // Colorer recurses while the chosen type asks for another start; a few rounds settle it.
        let mut chosen = None;
        for _ in 0..4 {
            let start = text_start(line, wanted)?;
            chosen = self.choose_type(file_name, &start, 0);
            let next = of(chosen, wanted);
            if next == wanted {
                break;
            }
            wanted = next;
        }
        Ok(chosen)
    }

    /// A type by its name.
    pub fn type_named(&self, name: &str) -> Option<TypeId> {
        self.type_names.get(name).copied()
    }

    /// The types a file may be chosen as, in their order (FarColorer's type menu's).
    pub fn listed_types(&self) -> &[TypeId] {
        &self.listed
    }

    pub fn file_type(&self, id: TypeId) -> &FileType {
        &self.types[id]
    }

    /// Whether the type's schemes were read (or could not be).
    pub fn is_loaded(&self, id: TypeId) -> bool {
        let file_type = &self.types[id];
        file_type.load_done || file_type.load_broken
    }

    /// The type's schemes read if they are not yet (`loadFileType`).
    pub fn load_type(&mut self, id: TypeId) {
        self.load_file_type(id);
    }

    pub fn scheme(&self, id: SchemeId) -> &Scheme {
        &self.schemes[id]
    }

    #[cfg(test)]
    pub fn scheme_named(&self, name: &str) -> Option<SchemeId> {
        self.scheme_names.get(name).copied()
    }

    pub fn node(&self, id: NodeRef) -> &Node {
        match id {
            NodeRef::Node(id) => &self.nodes[id],
            NodeRef::Owned(scheme, at) => &self.schemes[scheme].owned[at],
        }
    }

    pub fn region(&self, id: RegionId) -> &Region {
        &self.regions[id]
    }

    #[cfg(test)]
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    /// A region by its full name (`getRegion`).
    pub fn region_named(&self, name: &str) -> Option<RegionId> {
        self.region_names.get(name).copied()
    }

    /// Whether `region` is `ancestor` or has it among its parents (`hasParent`).
    pub fn region_has_parent(&self, region: RegionId, ancestor: RegionId) -> bool {
        let mut at = Some(region);
        while let Some(id) = at {
            if id == ancestor {
                return true;
            }
            at = self.regions[id].parent;
        }
        false
    }
}

/// Colorer's `chooseFileTypeCh` text: up to `lines` lines, `chars` characters of them in all, each
/// with a `\n` after it.
fn text_start(
    line: &mut dyn FnMut(usize) -> Result<Option<String>, ()>,
    (lines, chars): (i32, i32),
) -> Result<String, ()> {
    let (lines, chars) = (
        usize::try_from(lines).unwrap_or(0),
        usize::try_from(chars).unwrap_or(0),
    );
    let mut start = String::new();
    let mut total = 0;
    for index in 0..lines {
        let Some(text) = line(index)? else {
            break;
        };
        let taken: String = text.chars().take(chars - total).collect();
        total += taken.chars().count();
        start.push_str(&taken);
        start.push('\n');
        if total >= chars {
            break;
        }
    }
    Ok(start)
}

#[cfg(test)]
mod tests;
