use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};

use crate::FgdError;
use crate::lexer::{Tok, Token, lex, unescape_into};
use crate::model::{
    AutoVisGroup, AutoVisSubGroup, Choice, ClassKind, EntityClass, Fgd, FlagDef, Helper, IoDef, MAX_HELPERS_PER_CLASS,
    MetaIndex, Property, PropertyType, dedupe_classes,
};
use crate::named::NamedList;
use crate::text::decode_text;

const MAX_INCLUDE_DEPTH: usize = 32;
pub const MAX_FGD_FILE: u64 = 64 * 1024 * 1024;

fn read_fgd_file(path: &Path) -> std::io::Result<Vec<u8>> {
    let meta = fs::metadata(path)?;
    if !meta.is_file() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a regular file"));
    }
    if meta.len() > MAX_FGD_FILE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("larger than {} MB", MAX_FGD_FILE / (1024 * 1024)),
        ));
    }
    let bytes = fs::read(path)?;
    if bytes.len() as u64 > MAX_FGD_FILE {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "the file grew while it was read"));
    }
    Ok(bytes)
}

pub(crate) struct Loader {
    fgd: Fgd,
    loaded: HashSet<PathBuf>,
    stack: Vec<PathBuf>,
    include_dirs: Vec<PathBuf>,
    top_dirs: Vec<PathBuf>,
    meta: MetaIndex,
}

impl Loader {
    pub(crate) fn new(include_dirs: Vec<PathBuf>) -> Self {
        Self {
            fgd: Fgd::default(),
            loaded: HashSet::new(),
            stack: Vec::new(),
            include_dirs,
            top_dirs: Vec::new(),
            meta: MetaIndex::default(),
        }
    }

    pub(crate) fn finish(mut self) -> Fgd {
        dedupe_classes(&mut self.fgd.classes);
        self.fgd
    }

    pub(crate) fn load_top_level(&mut self, path: &Path) -> Result<(), FgdError> {
        let canonical = canonical(path);
        if self.loaded.contains(&canonical) {
            self.fgd.warnings.push(format!("{}: already loaded, skipped", path.display()));
            return Ok(());
        }
        let bytes = read_fgd_file(path).map_err(|source| FgdError::Io { path: path.to_owned(), source })?;
        self.parse_file_bytes(&bytes, path, canonical, 0);
        if let Some(dir) = path.parent()
            && !self.top_dirs.iter().any(|d| d == dir)
        {
            self.top_dirs.push(dir.to_path_buf());
        }
        Ok(())
    }

    fn parse_file_bytes(&mut self, bytes: &[u8], path: &Path, canonical: PathBuf, depth: usize) {
        self.loaded.insert(canonical.clone());
        let text = decode_text(bytes);
        let label = path.display().to_string();
        self.stack.push(canonical);
        self.parse_source(&text, &label, path.parent(), depth);
        self.stack.pop();
    }

    fn include(&mut self, name: &str, base_dir: Option<&Path>, depth: usize, at: &str) {
        if depth >= MAX_INCLUDE_DEPTH {
            self.fgd
                .warnings
                .push(format!("{at}: @include \"{name}\" ignored: nesting deeper than {MAX_INCLUDE_DEPTH}"));
            return;
        }
        let found = base_dir
            .into_iter()
            .chain(self.include_dirs.iter().map(PathBuf::as_path))
            .chain(self.top_dirs.iter().map(PathBuf::as_path))
            .find_map(|dir| find_in_dir(dir, name));
        let Some(path) = found else {
            let msg = if base_dir.is_none() && self.include_dirs.is_empty() && self.top_dirs.is_empty() {
                "no base directory to resolve it"
            } else {
                "file not found"
            };
            self.fgd.warnings.push(format!("{at}: @include \"{name}\" ignored: {msg}"));
            return;
        };
        let canonical = canonical(&path);
        if self.stack.contains(&canonical) {
            self.fgd.warnings.push(format!("{at}: recursive @include \"{name}\" ignored"));
            return;
        }
        if self.loaded.contains(&canonical) {
            return;
        }
        match read_fgd_file(&path) {
            Ok(bytes) => self.parse_file_bytes(&bytes, &path, canonical, depth + 1),
            Err(err) => self.fgd.warnings.push(format!("{at}: cannot read @include {}: {err}", path.display())),
        }
    }

    pub(crate) fn parse_source(&mut self, src: &str, label: &str, base_dir: Option<&Path>, depth: usize) {
        let lexed = lex(src);
        for (line, msg) in &lexed.diags {
            self.fgd.warnings.push(format!("{label}:{line}: {msg}"));
        }
        let mut parser = Parser { toks: &lexed.tokens, pos: 0, label, warnings: Vec::new() };
        while let Some(item) = parser.next_item() {
            self.fgd.warnings.append(&mut parser.warnings);
            match item {
                Item::Class(class) => self.fgd.classes.push(class),
                Item::Include { name, line } => {
                    let at = format!("{label}:{line}");
                    self.include(&name, base_dir, depth, &at);
                }
                Item::MapSize(min, max) => self.fgd.map_size = Some((min, max)),
                Item::MaterialExclusion(dirs) => {
                    for dir in dirs {
                        self.meta.add_exclusion(&mut self.fgd.material_exclusions, dir);
                    }
                }
                Item::AutoVisGroup(group) => {
                    self.meta.add_auto_visgroup(&mut self.fgd.auto_visgroups, group);
                }
                Item::Nothing => {}
            }
        }
        self.fgd.warnings.append(&mut parser.warnings);
    }
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn find_in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    let candidate = dir.join(name.trim().replace('\\', "/"));
    if candidate.is_file() {
        return Some(candidate);
    }
    let (parent, file) = (candidate.parent()?, candidate.file_name()?);
    fs::read_dir(parent)
        .ok()?
        .flatten()
        .find(|entry| entry.file_name().eq_ignore_ascii_case(file))
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
}

enum Item {
    Class(EntityClass),
    Include { name: String, line: u32 },
    MapSize(i32, i32),
    MaterialExclusion(Vec<String>),
    AutoVisGroup(AutoVisGroup),
    Nothing,
}

struct Parser<'t, 'a> {
    toks: &'t [Token<'a>],
    pos: usize,
    label: &'t str,
    warnings: Vec<String>,
}

fn describe(tok: Tok<'_>) -> String {
    fn clip(s: &str) -> &str {
        match s.char_indices().nth(32) {
            Some((i, _)) => s.get(..i).unwrap_or(s),
            None => s,
        }
    }
    match tok {
        Tok::Str { raw, .. } => format!("string \"{}\"", clip(raw)),
        Tok::Word(w) => format!("'{}'", clip(w)),
        Tok::Directive(d) => format!("'@{}'", clip(d)),
        Tok::Punct(c) => format!("'{}'", char::from(c)),
    }
}

fn push_raw(out: &mut String, raw: &str, escaped: bool) {
    if escaped {
        unescape_into(raw, out);
    } else {
        out.push_str(raw);
    }
}

impl<'a> Parser<'_, 'a> {
    fn peek(&self) -> Option<Tok<'a>> {
        self.toks.get(self.pos).map(|t| t.tok)
    }

    fn peek_at(&self, n: usize) -> Option<Tok<'a>> {
        self.toks.get(self.pos + n).map(|t| t.tok)
    }

    fn line(&self) -> u32 {
        self.toks.get(self.pos).or(self.toks.last()).map_or(1, |t| t.line)
    }

    fn at_punct(&self, c: u8) -> bool {
        self.peek() == Some(Tok::Punct(c))
    }

    fn eat_punct(&mut self, c: u8) -> bool {
        let hit = self.at_punct(c);
        if hit {
            self.pos += 1;
        }
        hit
    }

    fn at_directive_or_end(&self) -> bool {
        matches!(self.peek(), None | Some(Tok::Directive(_)))
    }

    fn warn(&mut self, line: u32, msg: impl Display) {
        self.warnings.push(format!("{}:{}: {}", self.label, line, msg));
    }

    fn skip_to_directive(&mut self) {
        while !self.at_directive_or_end() {
            self.pos += 1;
        }
    }

    fn skip_brackets(&mut self) {
        let mut depth = 0usize;
        while let Some(tok) = self.peek() {
            match tok {
                Tok::Directive(_) => return,
                Tok::Punct(b'[') => depth += 1,
                Tok::Punct(b']') => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        self.pos += 1;
                        return;
                    }
                }
                _ => {}
            }
            self.pos += 1;
        }
    }

    fn starts_body_item_at(&self, n: usize) -> bool {
        let Some(Tok::Word(w)) = self.peek_at(n) else {
            return false;
        };
        match self.peek_at(n + 1) {
            Some(Tok::Punct(b'(')) => true,
            Some(Tok::Word(_) | Tok::Str { .. }) => w.eq_ignore_ascii_case("input") || w.eq_ignore_ascii_case("output"),
            _ => false,
        }
    }

    fn concat_string(&mut self) -> String {
        let mut out = String::new();
        while let Some(Tok::Str { raw, escaped }) = self.peek() {
            self.pos += 1;
            push_raw(&mut out, raw, escaped);
            if !self.at_punct(b'+') {
                break;
            }
            if matches!(self.peek_at(1), Some(Tok::Str { .. })) {
                self.pos += 1;
            } else {
                let line = self.line();
                self.warn(line, "'+' not followed by a string");
                self.pos += 1;
                break;
            }
        }
        out
    }

    fn opt_value(&mut self) -> Option<String> {
        match self.peek() {
            Some(Tok::Str { .. }) => Some(self.concat_string()),
            Some(Tok::Word(_)) if !self.starts_body_item_at(0) => {
                let line = self.line();
                let mut out = String::new();
                while let Some(Tok::Word(w)) = self.peek() {
                    if self.line() != line || self.starts_body_item_at(0) {
                        break;
                    }
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(w);
                    self.pos += 1;
                }
                Some(out)
            }
            _ => None,
        }
    }

    fn single_value(&mut self) -> Option<String> {
        match self.peek() {
            Some(Tok::Str { .. }) => Some(self.concat_string()),
            Some(Tok::Word(w)) => {
                self.pos += 1;
                Some(w.to_owned())
            }
            _ => None,
        }
    }

    fn next_item(&mut self) -> Option<Item> {
        loop {
            let tok = self.peek()?;
            let line = self.line();
            if let Tok::Directive(name) = tok {
                self.pos += 1;
                return Some(self.directive(name, line));
            }
            self.warn(line, format_args!("unexpected {} at top level, skipped", describe(tok)));
            self.pos += 1;
            self.skip_to_directive();
        }
    }

    fn directive(&mut self, name: &str, line: u32) -> Item {
        if let Some(kind) = ClassKind::from_directive(name) {
            if let ClassKind::Other(other) = &kind {
                self.warn(line, format_args!("unknown class type @{other} treated as a point class"));
            }
            return self.class(kind).map_or(Item::Nothing, Item::Class);
        }
        let is = |n: &str| name.eq_ignore_ascii_case(n);
        if is("include") {
            match self.single_value() {
                Some(file) if !file.trim().is_empty() => Item::Include { name: file, line },
                _ => {
                    self.warn(line, "@include without a file name");
                    Item::Nothing
                }
            }
        } else if is("mapsize") {
            self.map_size(line)
        } else if is("materialexclusion") {
            self.material_exclusion(line)
        } else if is("autovisgroup") {
            self.auto_visgroup(line)
        } else if is("version") || is("gridnav") || is("exclude") {
            self.skip_to_directive();
            Item::Nothing
        } else {
            self.warn(line, format_args!("unknown directive @{name} skipped"));
            self.skip_to_directive();
            Item::Nothing
        }
    }

    fn map_size(&mut self, line: u32) -> Item {
        if !self.eat_punct(b'(') {
            self.warn(line, "@mapsize without '('");
            self.skip_to_directive();
            return Item::Nothing;
        }
        let mut numbers = Vec::with_capacity(2);
        let mut ok = true;
        loop {
            match self.peek() {
                Some(Tok::Punct(b')')) => {
                    self.pos += 1;
                    break;
                }
                Some(Tok::Punct(b',')) => self.pos += 1,
                Some(Tok::Word(w) | Tok::Str { raw: w, .. }) => {
                    self.pos += 1;
                    match w.trim().parse::<f64>() {
                        Ok(v) if v.is_finite() => numbers.push(v.round() as i32),
                        _ => ok = false,
                    }
                }
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        match *numbers.as_slice() {
            [min, max] if ok => Item::MapSize(min.min(max), min.max(max)),
            _ => {
                self.warn(line, "malformed @mapsize(min, max) ignored");
                self.skip_to_directive();
                Item::Nothing
            }
        }
    }

    fn name_list(&mut self, what: &str) -> Option<Vec<String>> {
        let line = self.line();
        if !self.eat_punct(b'[') {
            self.warn(line, format_args!("expected '[' in {what}"));
            return None;
        }
        let mut names = Vec::new();
        loop {
            match self.peek() {
                Some(Tok::Punct(b']')) => {
                    self.pos += 1;
                    break;
                }
                None | Some(Tok::Directive(_)) => {
                    let line = self.line();
                    self.warn(line, format_args!("unterminated list in {what}"));
                    break;
                }
                Some(Tok::Str { .. } | Tok::Word(_)) => {
                    if let Some(name) = self.single_value() {
                        names.push(name);
                    }
                }
                Some(Tok::Punct(b'[')) => {
                    let line = self.line();
                    self.warn(line, format_args!("unexpected nested list in {what} skipped"));
                    self.skip_brackets();
                }
                Some(tok) => {
                    let line = self.line();
                    self.warn(line, format_args!("unexpected {} in {what}", describe(tok)));
                    self.pos += 1;
                }
            }
        }
        Some(names)
    }

    fn material_exclusion(&mut self, line: u32) -> Item {
        match self.name_list("@MaterialExclusion") {
            Some(dirs) => Item::MaterialExclusion(dirs),
            None => {
                self.warn(line, "@MaterialExclusion skipped");
                self.skip_to_directive();
                Item::Nothing
            }
        }
    }

    fn auto_visgroup(&mut self, line: u32) -> Item {
        self.eat_punct(b'=');
        let Some(name) = self.single_value() else {
            self.warn(line, "@AutoVisGroup without a name skipped");
            self.skip_to_directive();
            return Item::Nothing;
        };
        if !self.eat_punct(b'[') {
            self.warn(line, format_args!("@AutoVisGroup \"{name}\" without '[' skipped"));
            self.skip_to_directive();
            return Item::Nothing;
        }
        let mut group = AutoVisGroup { name, groups: Vec::new() };
        loop {
            match self.peek() {
                Some(Tok::Punct(b']')) => {
                    self.pos += 1;
                    break;
                }
                None | Some(Tok::Directive(_)) => {
                    let line = self.line();
                    self.warn(line, format_args!("unterminated @AutoVisGroup \"{}\"", group.name));
                    break;
                }
                Some(Tok::Str { .. } | Tok::Word(_)) => {
                    let sub_name = self.single_value().unwrap_or_default();
                    let what = format!("@AutoVisGroup \"{}\"", group.name);
                    match self.name_list(&what) {
                        Some(classes) => group.groups.push(AutoVisSubGroup { name: sub_name, classes }),
                        None => break,
                    }
                }
                Some(tok) => {
                    let line = self.line();
                    self.warn(line, format_args!("unexpected {} in @AutoVisGroup", describe(tok)));
                    if tok == Tok::Punct(b'[') {
                        self.skip_brackets();
                    } else {
                        self.pos += 1;
                    }
                }
            }
        }
        Item::AutoVisGroup(group)
    }

    fn class(&mut self, kind: ClassKind) -> Option<EntityClass> {
        let mut helpers = Vec::new();
        let mut dropped_helpers = false;
        loop {
            match self.peek() {
                Some(Tok::Punct(b'=')) => {
                    self.pos += 1;
                    break;
                }
                Some(Tok::Word(w)) => {
                    let line = self.line();
                    self.pos += 1;
                    let args = if self.eat_punct(b'(') { self.helper_args(w) } else { Vec::new() };
                    if helpers.len() < MAX_HELPERS_PER_CLASS {
                        helpers.push(Helper { name: w.to_owned(), args });
                    } else if !dropped_helpers {
                        dropped_helpers = true;
                        self.warn(line, format_args!("more than {MAX_HELPERS_PER_CLASS} helpers, extra ones ignored"));
                    }
                }
                None | Some(Tok::Directive(_)) => {
                    let line = self.line();
                    self.warn(line, format_args!("@{} declaration without '=' skipped", kind.directive()));
                    return None;
                }
                Some(tok) => {
                    let line = self.line();
                    self.warn(line, format_args!("unexpected {} in class header", describe(tok)));
                    self.pos += 1;
                }
            }
        }

        let line = self.line();
        let name = match self.peek() {
            Some(Tok::Word(_) | Tok::Str { .. }) => self.single_value().unwrap_or_default(),
            _ => String::new(),
        };
        if name.trim().is_empty() {
            self.warn(line, format_args!("@{} without a class name skipped", kind.directive()));
            self.skip_to_directive();
            return None;
        }

        let mut class = EntityClass::new(kind, name);
        class.helpers = helpers;
        if self.eat_punct(b':') {
            class.description = self.opt_value().unwrap_or_default();
        }

        if !self.at_punct(b'[') {
            let line = self.line();
            self.warn(line, format_args!("expected '[' to open the body of class {}", class.name));
            while !self.at_directive_or_end() && !self.at_punct(b'[') {
                self.pos += 1;
            }
        }
        if self.eat_punct(b'[') {
            self.class_body(&mut class);
        }
        Some(class)
    }

    fn helper_args(&mut self, helper: &str) -> Vec<String> {
        let mut args = Vec::new();
        let mut cur = String::new();
        let mut depth = 1usize;
        loop {
            match self.peek() {
                None | Some(Tok::Directive(_)) | Some(Tok::Punct(b'=' | b'[' | b']' | b':')) => {
                    let line = self.line();
                    self.warn(line, format_args!("missing ')' after arguments of helper {helper}"));
                    break;
                }
                Some(Tok::Punct(b')')) => {
                    self.pos += 1;
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    cur.push(')');
                }
                Some(Tok::Punct(b',')) if depth == 1 => {
                    self.pos += 1;
                    args.push(std::mem::take(&mut cur));
                }
                Some(Tok::Punct(c)) => {
                    self.pos += 1;
                    if c == b'(' {
                        depth += 1;
                    }
                    cur.push(char::from(c));
                }
                Some(Tok::Word(w)) => {
                    self.pos += 1;
                    if !cur.is_empty() && !cur.ends_with('(') {
                        cur.push(' ');
                    }
                    cur.push_str(w);
                }
                Some(Tok::Str { raw, escaped }) => {
                    self.pos += 1;
                    if !cur.is_empty() && !cur.ends_with('(') {
                        cur.push(' ');
                    }
                    push_raw(&mut cur, raw, escaped);
                }
            }
        }
        if !cur.is_empty() || !args.is_empty() {
            args.push(cur);
        }
        args
    }

    fn class_body(&mut self, class: &mut EntityClass) {
        let mut props = NamedList::new(std::mem::take(&mut class.props));
        let mut inputs = NamedList::new(std::mem::take(&mut class.inputs));
        let mut outputs = NamedList::new(std::mem::take(&mut class.outputs));
        loop {
            match self.peek() {
                Some(Tok::Punct(b']')) => {
                    self.pos += 1;
                    break;
                }
                None | Some(Tok::Directive(_)) => {
                    let line = self.line();
                    self.warn(line, format_args!("body of class {} is not closed with ']'", class.name));
                    break;
                }
                Some(Tok::Word(w)) if self.starts_body_item_at(0) => {
                    let is_io = !matches!(self.peek_at(1), Some(Tok::Punct(b'(')));
                    if is_io {
                        let is_input = w.eq_ignore_ascii_case("input");
                        self.pos += 1;
                        let io = self.io_def();
                        if is_input { inputs.upsert(io) } else { outputs.upsert(io) }
                    } else if let Some(prop) = self.property(w) {
                        props.upsert(prop);
                    }
                }
                Some(tok) => {
                    let line = self.line();
                    self.warn(line, format_args!("unexpected {} in body of class {}", describe(tok), class.name));
                    self.recover_in_body();
                }
            }
        }
        class.props = props.into_vec();
        class.inputs = inputs.into_vec();
        class.outputs = outputs.into_vec();
    }

    fn recover_in_body(&mut self) {
        let start_line = self.line();
        let mut depth = 0usize;
        let mut first = true;
        while let Some(tok) = self.peek() {
            match tok {
                Tok::Directive(_) => return,
                Tok::Punct(b'[') => depth += 1,
                Tok::Punct(b']') => {
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                }
                Tok::Word(_) if !first && depth == 0 && self.line() > start_line && self.starts_body_item_at(0) => {
                    return;
                }
                _ => {}
            }
            self.pos += 1;
            first = false;
        }
    }

    fn property(&mut self, name: &str) -> Option<Property> {
        let line = self.line();
        self.pos += 2;
        let mut ty = String::new();
        let mut report = false;
        loop {
            match self.peek() {
                Some(Tok::Punct(b')')) => {
                    self.pos += 1;
                    break;
                }
                Some(Tok::Word(w) | Tok::Str { raw: w, .. }) => {
                    self.pos += 1;
                    let w = match w.strip_prefix('*') {
                        Some(rest) => {
                            report = true;
                            rest
                        }
                        None => w,
                    };
                    ty.push_str(w.trim());
                }
                _ => {
                    self.warn(line, format_args!("missing ')' after the type of property {name}"));
                    self.recover_in_body();
                    return None;
                }
            }
        }
        let kind = if ty.is_empty() {
            self.warn(line, format_args!("property {name} has no type"));
            PropertyType::Other(String::new())
        } else {
            PropertyType::from_name(&ty)
        };
        let mut prop = Property::new(name, kind);
        prop.report = report;

        while let Some(Tok::Word(w)) = self.peek() {
            if matches!(self.peek_at(1), Some(Tok::Punct(b'('))) {
                break;
            }
            if w.eq_ignore_ascii_case("readonly") {
                prop.readonly = true;
            } else if w.eq_ignore_ascii_case("report") {
                prop.report = true;
            } else {
                break;
            }
            self.pos += 1;
        }

        if self.eat_punct(b':') {
            prop.display_name = self.opt_value().unwrap_or_default();
            if self.eat_punct(b':') {
                prop.default = self.opt_value().filter(|v| !v.is_empty());
                if self.eat_punct(b':') {
                    prop.description = self.opt_value().unwrap_or_default();
                    while self.eat_punct(b':') {
                        self.opt_value();
                    }
                }
            }
        }

        if self.eat_punct(b'=') {
            if self.eat_punct(b'[') {
                if prop.kind == PropertyType::Flags {
                    prop.flags = self.flag_list(name);
                } else {
                    prop.choices = self.choice_list(name);
                }
            } else {
                let line = self.line();
                self.warn(line, format_args!("expected '[' after '=' in property {name}"));
            }
        }
        Some(prop)
    }

    fn list_junk(&mut self, prop: &str) -> bool {
        let line = self.line();
        match self.peek() {
            None | Some(Tok::Directive(_)) => {
                self.warn(line, format_args!("unterminated list of property {prop}"));
                false
            }
            Some(Tok::Punct(b'[')) => {
                self.warn(line, format_args!("unexpected nested list in property {prop} skipped"));
                self.skip_brackets();
                true
            }
            Some(tok) => {
                self.warn(line, format_args!("unexpected {} in list of property {prop}", describe(tok)));
                self.pos += 1;
                true
            }
        }
    }

    fn choice_list(&mut self, prop: &str) -> Vec<Choice> {
        let mut choices = Vec::new();
        loop {
            match self.peek() {
                Some(Tok::Punct(b']')) => {
                    self.pos += 1;
                    break;
                }
                Some(Tok::Str { .. } | Tok::Word(_)) => {
                    let value = self.single_value().unwrap_or_default();
                    let display =
                        if self.eat_punct(b':') { self.single_value().unwrap_or_default() } else { value.clone() };
                    while self.eat_punct(b':') {
                        self.single_value();
                    }
                    choices.push(Choice { value, display });
                }
                _ => {
                    if !self.list_junk(prop) {
                        break;
                    }
                }
            }
        }
        choices
    }

    fn flag_list(&mut self, prop: &str) -> Vec<FlagDef> {
        let mut flags: Vec<FlagDef> = Vec::new();
        let mut positions: HashMap<u32, usize> = HashMap::new();
        loop {
            match self.peek() {
                Some(Tok::Punct(b']')) => {
                    self.pos += 1;
                    break;
                }
                Some(Tok::Str { .. } | Tok::Word(_)) => {
                    let line = self.line();
                    let raw_value = self.single_value().unwrap_or_default();
                    let display =
                        if self.eat_punct(b':') { self.single_value().unwrap_or_default() } else { String::new() };
                    let default_on =
                        if self.eat_punct(b':') { self.single_value().is_some_and(|v| parse_bool(&v)) } else { false };
                    while self.eat_punct(b':') {
                        self.single_value();
                    }
                    match parse_flag_value(&raw_value) {
                        Some(value) => {
                            let flag = FlagDef { value, display, default_on };
                            match positions.get(&value).and_then(|&i| flags.get_mut(i)) {
                                Some(slot) => *slot = flag,
                                None => {
                                    positions.insert(value, flags.len());
                                    flags.push(flag);
                                }
                            }
                        }
                        None => {
                            self.warn(line, format_args!("invalid flag value '{raw_value}' in property {prop} ignored"))
                        }
                    }
                }
                _ => {
                    if !self.list_junk(prop) {
                        break;
                    }
                }
            }
        }
        flags
    }

    fn io_def(&mut self) -> IoDef {
        let line = self.line();
        let name = self.single_value().unwrap_or_default();
        let mut ty = String::new();
        if self.eat_punct(b'(') {
            loop {
                match self.peek() {
                    Some(Tok::Punct(b')')) => {
                        self.pos += 1;
                        break;
                    }
                    Some(Tok::Word(w) | Tok::Str { raw: w, .. }) => {
                        self.pos += 1;
                        ty.push_str(w.trim());
                    }
                    _ => {
                        self.warn(line, format_args!("missing ')' after the type of {name}"));
                        break;
                    }
                }
            }
        }
        let arg_type = if ty.is_empty() { PropertyType::Void } else { PropertyType::from_name(&ty) };
        let description = if self.eat_punct(b':') { self.opt_value().unwrap_or_default() } else { String::new() };
        IoDef { name, arg_type, description }
    }
}

fn parse_flag_value(text: &str) -> Option<u32> {
    let text = text.trim();
    if let Ok(v) = text.parse::<u32>() {
        return Some(v);
    }
    if let Ok(v) = text.parse::<i32>() {
        return Some(v.cast_unsigned());
    }
    match text.parse::<f64>() {
        Ok(v) if v.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&v) => Some(v as u32),
        _ => None,
    }
}

fn parse_bool(text: &str) -> bool {
    let text = text.trim();
    match text.parse::<f64>() {
        Ok(v) => v != 0.0,
        Err(_) => text.eq_ignore_ascii_case("true") || text.eq_ignore_ascii_case("yes"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Fgd {
        Fgd::parse(src, None)
    }

    fn only_class(fgd: &Fgd) -> &EntityClass {
        assert_eq!(fgd.classes.len(), 1, "classes: {:#?}\nwarnings: {:#?}", fgd.classes, fgd.warnings);
        &fgd.classes[0]
    }

    #[test]
    fn empty_and_comment_only() {
        assert_eq!(parse(""), Fgd::default());
        let fgd = parse("// nothing\n   // here\n");
        assert!(fgd.classes.is_empty() && fgd.warnings.is_empty());
    }

    #[test]
    fn all_class_kinds() {
        let src = r#"
            @BaseClass = B []
            @PointClass = p []
            @SolidClass = s []
            @NPCClass = n []
            @KeyFrameClass = k []
            @MoveClass = m []
            @FilterClass = f []
            @OverrideClass = p []
            @ExtendClass = s []
            @pointclass = lower []
        "#;
        let fgd = parse(src);
        let kinds: Vec<_> = fgd.classes.iter().map(|c| c.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                ClassKind::Base,
                ClassKind::Point,
                ClassKind::Solid,
                ClassKind::Npc,
                ClassKind::KeyFrame,
                ClassKind::Move,
                ClassKind::Filter,
                ClassKind::Override,
                ClassKind::Extend,
                ClassKind::Point,
            ]
        );
        assert!(fgd.warnings.is_empty(), "{:?}", fgd.warnings);
    }

    #[test]
    fn unknown_class_kind_is_point_with_warning() {
        let fgd = parse("@PathClass = path_track_2 : \"x\" []");
        let class = only_class(&fgd);
        assert_eq!(class.kind, ClassKind::Other("PathClass".into()));
        assert!(class.kind.is_point());
        assert_eq!(fgd.warnings.len(), 1);
    }

    #[test]
    fn class_header_helpers() {
        let src = r#"@PointClass base(Targetname, Parentname) color(255 128 0) size(-16 -16 0, 16 16 72)
            studio("models/editor/playerstart.mdl") studioprop() iconsprite("editor/light.vmt")
            sphere(radius) line(255 255 255, targetname, target) halfgridsnap
            frustum(lightfov, nearz, farz, lightcolor, -1) cylinder(255 255 255, targetname, target, radius, targetname, targetname, radius)
            = demo : "Demo" + " entity" []"#;
        let fgd = parse(src);
        let class = only_class(&fgd);
        assert!(fgd.warnings.is_empty(), "{:?}", fgd.warnings);
        assert_eq!(class.description, "Demo entity");
        let h = |i: usize| (class.helpers[i].name.as_str(), class.helpers[i].args.clone());
        let v = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(h(0), ("base", v(&["Targetname", "Parentname"])));
        assert_eq!(h(1), ("color", v(&["255 128 0"])));
        assert_eq!(h(2), ("size", v(&["-16 -16 0", "16 16 72"])));
        assert_eq!(h(3), ("studio", v(&["models/editor/playerstart.mdl"])));
        assert_eq!(h(4), ("studioprop", v(&[])));
        assert_eq!(h(5), ("iconsprite", v(&["editor/light.vmt"])));
        assert_eq!(h(6), ("sphere", v(&["radius"])));
        assert_eq!(h(7), ("line", v(&["255 255 255", "targetname", "target"])));
        assert_eq!(h(8), ("halfgridsnap", v(&[])));
        assert_eq!(h(9), ("frustum", v(&["lightfov", "nearz", "farz", "lightcolor", "-1"])));
        assert_eq!(class.helpers[10].args.len(), 7);
        assert_eq!(class.bases().collect::<Vec<_>>(), ["Targetname", "Parentname"]);
    }

    #[test]
    fn helper_with_empty_and_nested_args() {
        let fgd = parse("@PointClass foo(a, ) bar(f(1 2), x) baz(\"\") = c []");
        let class = only_class(&fgd);
        assert_eq!(class.helpers[0].args, ["a", ""]);
        assert_eq!(class.helpers[1].args, ["f(1 2)", "x"]);
        assert!(class.helpers[2].args.is_empty());
    }

    #[test]
    fn helper_missing_paren_recovers() {
        let fgd = parse("@PointClass studio(\"x.mdl\" = c [ a(string) ]");
        let class = only_class(&fgd);
        assert_eq!(class.name, "c");
        assert_eq!(class.helpers[0].args, ["x.mdl"]);
        assert_eq!(class.props.len(), 1);
        assert_eq!(fgd.warnings.len(), 1);
    }

    #[test]
    fn full_property() {
        let src = r#"@PointClass = c : "C"
        [
            health(integer) : "Health" : 100 : "Hit points." + " More."
            model(*studio) readonly : "Model" : "models/a.mdl"
            note(string) report : "Note"
            angles(angle) : "Angles" : "0 0 0" : "Orientation."
            targetname(target_source) : "Name" : : "Name of this entity."
            rendercolor(color255) : "Color" : 255 255 255 : "Tint."
            empty(string) : "Empty" : "" : ""
            bare(float)
            weird(MyCustomType) : "Weird"
        ]"#;
        let fgd = parse(src);
        assert!(fgd.warnings.is_empty(), "{:?}", fgd.warnings);
        let class = only_class(&fgd);
        let p = |n: &str| class.property(n).unwrap();

        let health = p("health");
        assert_eq!(health.kind, PropertyType::Integer);
        assert_eq!(health.display_name, "Health");
        assert_eq!(health.default.as_deref(), Some("100"));
        assert_eq!(health.description, "Hit points. More.");

        let model = p("model");
        assert_eq!(model.kind, PropertyType::Studio);
        assert!(model.report && model.readonly);
        assert_eq!(model.default.as_deref(), Some("models/a.mdl"));

        assert!(p("note").report);
        assert_eq!(p("angles").default.as_deref(), Some("0 0 0"));
        let target = p("targetname");
        assert_eq!(target.kind, PropertyType::TargetSource);
        assert_eq!(target.default, None);
        assert_eq!(target.description, "Name of this entity.");
        assert_eq!(p("rendercolor").default.as_deref(), Some("255 255 255"));
        assert_eq!(p("rendercolor").description, "Tint.");
        assert_eq!(p("empty").default, None);
        assert_eq!(p("bare").kind, PropertyType::Float);
        assert_eq!(p("bare").display_name, "");
        assert_eq!(p("weird").kind, PropertyType::Other("MyCustomType".into()));
    }

    #[test]
    fn property_named_like_keywords() {
        let src = r#"@PointClass = c [
            input(string) : "Input"
            output(string) : "Output"
            readonly(integer) readonly : "RO"
            input Fire(void)
        ]"#;
        let fgd = parse(src);
        assert!(fgd.warnings.is_empty(), "{:?}", fgd.warnings);
        let class = only_class(&fgd);
        assert_eq!(class.props.len(), 3);
        assert!(class.property("readonly").unwrap().readonly);
        assert_eq!(class.inputs.len(), 1);
    }

    #[test]
    fn choices_and_flags() {
        let src = r#"@PointClass = c [
            mode(choices) : "Mode" : 0 : "How." =
            [
                0 : "Off"
                1: "On"
                -1 : "Default"
                "custom" : "Custom" : "extra description"
            ]
            spawnflags(Flags) =
            [
                1 : "Start on" : 1
                2 : "Silent" : 0
                4: "No default"
                8 :"Quoted" : "1"
            ]
            sounds(sound) : "Sound" : "a.wav" = [ "a.wav" : "A" "b.wav" : "B" ]
            inline(flags) = [ 1 : "x" : 1 2 : "y" : 0 ]
        ]"#;
        let fgd = parse(src);
        assert!(fgd.warnings.is_empty(), "{:?}", fgd.warnings);
        let class = only_class(&fgd);
        let mode = class.property("mode").unwrap();
        let values: Vec<_> = mode.choices.iter().map(|c| (c.value.as_str(), c.display.as_str())).collect();
        assert_eq!(values, [("0", "Off"), ("1", "On"), ("-1", "Default"), ("custom", "Custom")]);
        assert_eq!(mode.description, "How.");

        let flags = class.property("spawnflags").unwrap();
        assert_eq!(flags.kind, PropertyType::Flags);
        let bits: Vec<_> = flags.flags.iter().map(|f| (f.value, f.display.as_str(), f.default_on)).collect();
        assert_eq!(bits, [(1, "Start on", true), (2, "Silent", false), (4, "No default", false), (8, "Quoted", true)]);
        assert_eq!(flags.default_flags(), 9);

        let sounds = class.property("sounds").unwrap();
        assert_eq!(sounds.choices.len(), 2);
        assert_eq!(sounds.default.as_deref(), Some("a.wav"));

        let inline = class.property("inline").unwrap();
        assert_eq!(inline.flags.len(), 2);
        assert_eq!(inline.default_flags(), 1);
    }

    #[test]
    fn bad_flag_value_warns() {
        let fgd = parse("@PointClass = c [ spawnflags(flags) = [ abc : \"x\" : 0  2 : \"y\" : 1 ] ]");
        let class = only_class(&fgd);
        assert_eq!(class.props[0].flags.len(), 1);
        assert_eq!(fgd.warnings.len(), 1);
    }

    #[test]
    fn inputs_and_outputs() {
        let src = r#"@PointClass = c [
            input Open(void) : "Open it."
            input SetSpeed(float) : "Speed." + " Units per second."
            input AlternativeSorting(bool) : "Sort."
            output OnOpen(void) : "Opened."
            output OnHealthChanged(float)
            input Open(void) : "Redefined."
        ]"#;
        let fgd = parse(src);
        assert!(fgd.warnings.is_empty(), "{:?}", fgd.warnings);
        let class = only_class(&fgd);
        assert_eq!(class.inputs.len(), 3);
        assert_eq!(class.inputs[0].description, "Redefined.");
        assert_eq!(class.inputs[1].arg_type, PropertyType::Float);
        assert_eq!(class.inputs[1].description, "Speed. Units per second.");
        assert_eq!(class.inputs[2].arg_type, PropertyType::Boolean);
        assert_eq!(class.outputs.len(), 2);
        assert_eq!(class.outputs[1].description, "");
    }

    #[test]
    fn duplicate_property_in_body_replaced_in_place() {
        let fgd = parse("@PointClass = c [ a(integer) : \"A\" : 1 \n b(string) \n a(integer) : \"A2\" : 2 ]");
        let class = only_class(&fgd);
        assert_eq!(class.props.len(), 2);
        assert_eq!(class.props[0].display_name, "A2");
    }

    #[test]
    fn map_size_material_exclusion_autovisgroup() {
        let src = r#"
            @mapsize(-16384, 16384)
            @MaterialExclusion [ "console" "debug" ]
            @MaterialExclusion [ "Debug" "tools" ]
            @AutoVisGroup = "Brushes"
            [
                "Triggers" [ "trigger_once" "trigger_multiple" ]
                "Tool Brushes" [ "func_areaportal" ]
            ]
            @AutoVisGroup = "brushes" [ "Triggers" [ "trigger_push" "trigger_once" ] "Other" [ "x" ] ]
            @version(9)
            @gridnav(64, 32, 32, 16384)
        "#;
        let fgd = parse(src);
        assert!(fgd.warnings.is_empty(), "{:?}", fgd.warnings);
        assert_eq!(fgd.map_size, Some((-16384, 16384)));
        assert_eq!(fgd.material_exclusions, ["console", "debug", "tools"]);
        assert_eq!(fgd.auto_visgroups.len(), 1);
        let group = &fgd.auto_visgroups[0];
        assert_eq!(group.groups.len(), 3);
        assert_eq!(group.groups[0].classes, ["trigger_once", "trigger_multiple", "trigger_push"]);
    }

    #[test]
    fn malformed_map_size() {
        let fgd = parse("@mapsize(a, b) @PointClass = c []");
        assert_eq!(fgd.map_size, None);
        assert_eq!(fgd.classes.len(), 1);
        assert_eq!(fgd.warnings.len(), 1);
        let fgd = parse("@mapsize(100, -100)");
        assert_eq!(fgd.map_size, Some((-100, 100)));
    }

    #[test]
    fn multiline_descriptions_and_escapes() {
        let src = "@SolidClass = func_ladder :\n\t\"Ladder.\" +\n\t\" Climb it.\\nNew line.\"\n[\n]";
        let fgd = parse(src);
        assert_eq!(only_class(&fgd).description, "Ladder. Climb it.\nNew line.");
    }

    #[test]
    fn dangling_plus_warns() {
        let fgd = parse("@PointClass = c : \"a\" + [ ]");
        assert_eq!(only_class(&fgd).description, "a");
        assert_eq!(fgd.warnings.len(), 1);
    }

    #[test]
    fn recovery_from_garbage() {
        let src = r#"
            garbage at top level
            @Unknown(1, 2) [ stuff ]
            @PointClass base(A) = c : "C"
            [
                good1(string) : "Good 1"
                ??? broken : line : [ nested ] stuff
                good2(integer) : "Good 2" : 5
                bad(string : "missing paren"
                good3(float) : "Good 3"
                : stray
                input Fire(void) : "Fire"
            ]
            @PointClass = d [ x(string) ]
        "#;
        let fgd = parse(src);
        assert_eq!(fgd.classes.len(), 2, "{:#?}", fgd.warnings);
        let c = &fgd.classes[0];
        let names: Vec<_> = c.props.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["good1", "good2", "good3"]);
        assert_eq!(c.property("good2").unwrap().default.as_deref(), Some("5"));
        assert_eq!(c.inputs.len(), 1);
        assert_eq!(fgd.classes[1].props.len(), 1);
        assert!(fgd.warnings.len() >= 4, "{:#?}", fgd.warnings);
        assert!(fgd.warnings.iter().all(|w| w.starts_with("<input>:")));
    }

    #[test]
    fn unterminated_body_and_missing_parts() {
        let src = r#"
            @PointClass = a [ x(string) : "X"
            @PointClass = b : "no body"
            @PointClass base(A)
            @PointClass =
            @PointClass = c [ y(integer) ]
        "#;
        let fgd = parse(src);
        let names: Vec<_> = fgd.classes.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);
        assert_eq!(fgd.classes[0].props.len(), 1);
        assert_eq!(fgd.warnings.len(), 4, "{:#?}", fgd.warnings);
    }

    #[test]
    fn include_without_base_dir_warns() {
        let fgd = parse("@include \"base.fgd\"\n@PointClass = c []");
        assert_eq!(fgd.classes.len(), 1);
        assert_eq!(fgd.warnings.len(), 1);
        assert!(fgd.warnings[0].contains("base.fgd"));
    }

    #[test]
    fn flag_and_bool_parsing() {
        assert_eq!(parse_flag_value("1024"), Some(1024));
        assert_eq!(parse_flag_value("-2147483648"), Some(0x8000_0000));
        assert_eq!(parse_flag_value("4294967295"), Some(u32::MAX));
        assert_eq!(parse_flag_value("8.0"), Some(8));
        assert_eq!(parse_flag_value("1.5"), None);
        assert_eq!(parse_flag_value("x"), None);
        assert!(parse_bool("1") && parse_bool("TRUE") && !parse_bool("0") && !parse_bool("no"));
    }

    #[test]
    fn hostile_input_does_not_panic() {
        const PIECES: [&str; 24] = [
            "@PointClass",
            "@BaseClass",
            "@include",
            "@AutoVisGroup",
            "@mapsize",
            "(",
            ")",
            "[",
            "]",
            ":",
            "=",
            ",",
            "+",
            "\"str\"",
            "\"unterminated\n",
            "word",
            "base",
            "flags",
            "input",
            "output",
            "\n",
            "-1",
            "// c\n",
            "\\",
        ];
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..300 {
            let mut src = String::new();
            for _ in 0..200 {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let piece = PIECES[(state % PIECES.len() as u64) as usize];
                src.push_str(piece);
                src.push(' ');
            }
            let fgd = Fgd::parse(&src, None);
            let _ = fgd.resolve();
        }
        let bytes: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
        let text = decode_text(&bytes);
        let _ = Fgd::parse(&text, None).resolve();
    }
}
