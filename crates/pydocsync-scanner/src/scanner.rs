use crate::string::scan_string;
use crate::{ByteRange, FileSummary, FunctionItem, ParameterRecord};

type TextRange = ByteRange;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScopeKind {
    Function,
    Class,
}

#[derive(Clone, Copy, Debug)]
struct Scope {
    item_index: Option<usize>,
    header_indent: usize,
    kind: ScopeKind,
}

#[derive(Clone, Copy, Debug)]
struct Line {
    start: usize,
    content_end: usize,
}

pub(crate) struct Scanner<'a> {
    source: &'a str,
    bytes: &'a [u8],
    lines: Vec<Line>,
    line_depths: Vec<u32>,
    summary: FileSummary,
    scopes: Vec<Scope>,
}

impl<'a> Scanner<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        let lines = line_ranges(source.as_bytes());
        Self {
            source,
            bytes: source.as_bytes(),
            line_depths: line_start_depths(source.as_bytes(), &lines),
            lines,
            summary: FileSummary::default(),
            scopes: Vec::new(),
        }
    }

    pub(crate) fn scan(mut self) -> FileSummary {
        let mut line_index = 0;
        while line_index < self.lines.len() {
            let line = self.lines[line_index];
            let first = self.first_non_ws(line.start, line.content_end);
            if first >= line.content_end || self.bytes[first] == b'#' {
                line_index += 1;
                continue;
            }

            if self.line_depths[line_index] > 0 {
                line_index += 1;
                continue;
            }

            let indent = first - line.start;
            self.close_dedented_scopes(indent);

            if let Some(next_line) = self.skip_multiline_string_literal(line_index) {
                line_index = next_line;
                continue;
            }

            if self.bytes[first] == b'@' {
                line_index += 1;
                continue;
            }

            if self.at_keyword(first, b"async")
                && let Some(def_pos) = self.after_keyword_ws(first, b"async")
                && self.at_keyword(def_pos, b"def")
                && let Some(next_line) = self.parse_function(first, def_pos, indent, line_index, true)
            {
                line_index = next_line;
                continue;
            }

            if self.at_keyword(first, b"def")
                && let Some(next_line) = self.parse_function(first, first, indent, line_index, false)
            {
                line_index = next_line;
                continue;
            }

            if self.at_keyword(first, b"class")
                && let Some(next_line) = self.parse_class(first, indent, line_index)
            {
                line_index = next_line;
                continue;
            }

            self.scan_function_fact(first, line.content_end);
            line_index += 1;
        }

        self.summary
    }

    fn parse_function(
        &mut self,
        header_start: usize,
        def_pos: usize,
        indent: usize,
        line_index: usize,
        is_async: bool,
    ) -> Option<usize> {
        let mut pos = def_pos + 3;
        pos = self.skip_ws(pos, self.bytes.len());
        let name_start = pos;
        pos = self.scan_identifier(pos)?;
        let name_end = pos;
        pos = self.skip_ws(pos, self.bytes.len());
        if self.bytes.get(pos) == Some(&b'[') {
            pos = self.scan_balanced(pos, b'[', b']')?;
            pos = self.skip_ws(pos, self.bytes.len());
        }
        if self.bytes.get(pos) != Some(&b'(') {
            return None;
        }
        let params_start = pos;
        let params_end = self.scan_balanced(pos, b'(', b')')?;
        let colon = self.find_header_colon(params_end)?;
        let colon_line_index = self.line_index_for_offset(colon)?;
        let return_annotation_range = self.find_return_annotation(params_end, colon);
        let docstring_range = self.find_suite_docstring(colon + 1, indent, colon_line_index);
        let is_method = self.is_inside_class(indent);
        let parameters = self.parse_signature_parameters(params_start, params_end, is_method);
        let item_index = self.summary.items.len();
        let name = self.source[name_start..name_end].to_string();
        self.summary.items.push(FunctionItem {
            name,
            name_range: TextRange::new(name_start, name_end),
            header_range: TextRange::new(header_start, colon + 1),
            params_range: TextRange::new(params_start, params_end),
            return_annotation_range,
            docstring_range,
            is_async,
            is_method,
            parameters,
            has_return_value: false,
            has_yield: false,
        });
        self.scopes.push(Scope {
            item_index: Some(item_index),
            header_indent: indent,
            kind: ScopeKind::Function,
        });
        Some(line_index + 1)
    }

    fn parse_class(&mut self, header_start: usize, indent: usize, line_index: usize) -> Option<usize> {
        let mut pos = header_start + 5;
        pos = self.skip_ws(pos, self.bytes.len());
        pos = self.scan_identifier(pos)?;
        self.find_header_colon(pos)?;
        self.scopes.push(Scope {
            item_index: None,
            header_indent: indent,
            kind: ScopeKind::Class,
        });
        Some(line_index + 1)
    }

    fn find_suite_docstring(&self, after_colon: usize, header_indent: usize, line_index: usize) -> Option<TextRange> {
        let header_line = self.lines[line_index];
        let same_line_first = self.first_non_ws(after_colon, header_line.content_end);
        if same_line_first < header_line.content_end
            && self.bytes[same_line_first] != b'#'
            && let Some(lit) = scan_string(self.bytes, same_line_first)
            && lit.is_terminated
        {
            return Some(lit.range);
        }

        for line in self.lines.iter().skip(line_index + 1) {
            let first = self.first_non_ws(line.start, line.content_end);
            if first >= line.content_end || self.bytes[first] == b'#' {
                continue;
            }
            let indent = first - line.start;
            if indent <= header_indent {
                return None;
            }
            return scan_string(self.bytes, first)
                .filter(|lit| lit.is_terminated)
                .map(|lit| lit.range);
        }
        None
    }

    fn scan_function_fact(&mut self, first: usize, line_end: usize) {
        let Some(function_scope_index) = self.current_function_scope_index() else {
            return;
        };

        if self.at_keyword(first, b"return") {
            let expr_start = self.skip_ws(first + 6, line_end);
            if expr_start < line_end
                && self.bytes[expr_start] != b'#'
                && !self.is_none_return_expr(expr_start, line_end)
            {
                if let Some(item_index) = self.scopes[function_scope_index].item_index {
                    self.set_function_return_value(item_index);
                }
            }
            return;
        }

        if self.at_keyword(first, b"yield") {
            if let Some(item_index) = self.scopes[function_scope_index].item_index {
                self.set_function_yield(item_index);
            }
        }
    }

    fn is_none_return_expr(&self, start: usize, line_end: usize) -> bool {
        if !self.bytes[start..line_end].starts_with(b"None") {
            return false;
        }
        let after_none = start + 4;
        if after_none < line_end && is_ident_continue_byte(self.bytes[after_none]) {
            return false;
        }
        let rest = self.skip_ws(after_none, line_end);
        rest >= line_end || self.bytes[rest] == b'#'
    }

    fn find_return_annotation(&self, params_end: usize, colon: usize) -> Option<TextRange> {
        let mut pos = params_end;
        while pos + 1 < colon {
            if self.bytes[pos] == b'-' && self.bytes[pos + 1] == b'>' {
                let end = self.trim_end_ws(pos, colon);
                return Some(TextRange::new(pos, end));
            }
            pos += 1;
        }
        None
    }

    fn parse_signature_parameters(
        &self,
        params_start: usize,
        params_end: usize,
        is_method: bool,
    ) -> Vec<ParameterRecord> {
        if params_start + 1 >= params_end {
            return Vec::new();
        }

        let mut parameters = Vec::new();
        let mut segment_start = params_start + 1;
        let mut pos = segment_start;
        let mut depth = 0usize;
        while pos < params_end - 1 {
            if let Some(lit) = scan_string(self.bytes, pos) {
                pos = lit.range.end();
                continue;
            }
            match self.bytes[pos] {
                b'#' => pos = self.line_end_from(pos),
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth = depth.saturating_sub(1),
                b',' if depth == 0 => {
                    self.push_parameter_record(segment_start, pos, is_method, &mut parameters);
                    segment_start = pos + 1;
                }
                _ => {}
            }
            pos += 1;
        }
        self.push_parameter_record(segment_start, params_end - 1, is_method, &mut parameters);
        parameters
    }

    fn push_parameter_record(&self, start: usize, end: usize, is_method: bool, parameters: &mut Vec<ParameterRecord>) {
        let mut start = self.skip_ws_and_comments(start, end);
        let end = self.trim_end_ws(start, end);
        if start >= end {
            return;
        }
        if self.bytes[start] == b'/' || self.bytes[start] == b'*' && start + 1 == end {
            return;
        }

        let is_kwarg = self.bytes.get(start..start + 2) == Some(b"**");
        let is_vararg = !is_kwarg && self.bytes.get(start) == Some(&b'*');
        if is_kwarg {
            start += 2;
        } else if is_vararg {
            start += 1;
        }
        start = self.skip_ws(start, end);

        let Some(name_end) = self.scan_identifier(start) else {
            return;
        };
        if name_end > end {
            return;
        }

        let bare_name = self.source[start..name_end].to_string();
        let name = if is_kwarg {
            format!("**{bare_name}")
        } else if is_vararg {
            format!("*{bare_name}")
        } else {
            bare_name.clone()
        };
        let annotation_range = self.find_top_level_after(start, end, b':').map(|colon| {
            let annotation_start = self.skip_ws(colon + 1, end);
            let annotation_end = self.find_top_level_after(annotation_start, end, b'=').unwrap_or(end);
            TextRange::new(annotation_start, self.trim_end_ws(annotation_start, annotation_end))
        });
        let default_range = self.find_top_level_after(start, end, b'=').map(|equals| {
            let default_start = self.skip_ws(equals + 1, end);
            TextRange::new(default_start, self.trim_end_ws(default_start, end))
        });
        let is_implicit_receiver = is_method
            && parameters.is_empty()
            && !is_vararg
            && !is_kwarg
            && matches!(bare_name.as_str(), "self" | "cls");

        parameters.push(ParameterRecord {
            name,
            bare_name,
            name_range: TextRange::new(start, name_end),
            annotation_range: annotation_range.filter(|range| range.start() < range.end()),
            default_range: default_range.filter(|range| range.start() < range.end()),
            is_vararg,
            is_kwarg,
            is_implicit_receiver,
        });
    }

    fn find_top_level_after(&self, start: usize, end: usize, needle: u8) -> Option<usize> {
        let mut pos = start;
        let mut depth = 0usize;
        while pos < end {
            if let Some(lit) = scan_string(self.bytes, pos) {
                pos = lit.range.end();
                continue;
            }
            match self.bytes[pos] {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth = depth.saturating_sub(1),
                byte if byte == needle && depth == 0 => return Some(pos),
                _ => {}
            }
            pos += 1;
        }
        None
    }

    fn find_header_colon(&self, start: usize) -> Option<usize> {
        let mut pos = start;
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        while pos < self.bytes.len() {
            if let Some(lit) = scan_string(self.bytes, pos) {
                pos = lit.range.end();
                continue;
            }
            match self.bytes[pos] {
                b'#' => pos = self.line_end_from(pos),
                b'\n' | b'\r' => pos += 1,
                b'(' => {
                    paren_depth += 1;
                    pos += 1;
                }
                b')' => {
                    paren_depth = paren_depth.saturating_sub(1);
                    pos += 1;
                }
                b'[' => {
                    bracket_depth += 1;
                    pos += 1;
                }
                b']' => {
                    bracket_depth = bracket_depth.saturating_sub(1);
                    pos += 1;
                }
                b'{' => {
                    brace_depth += 1;
                    pos += 1;
                }
                b'}' => {
                    brace_depth = brace_depth.saturating_sub(1);
                    pos += 1;
                }
                b':' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => return Some(pos),
                _ => pos += 1,
            }
        }
        None
    }

    fn scan_balanced(&self, start: usize, open: u8, close: u8) -> Option<usize> {
        if self.bytes.get(start) != Some(&open) {
            return None;
        }
        let mut depth = 1usize;
        let mut pos = start + 1;
        while pos < self.bytes.len() {
            if let Some(lit) = scan_string(self.bytes, pos) {
                pos = lit.range.end();
                continue;
            }
            match self.bytes[pos] {
                b'#' => pos = self.line_end_from(pos),
                b if b == open => {
                    depth += 1;
                    pos += 1;
                }
                b if b == close => {
                    depth -= 1;
                    pos += 1;
                    if depth == 0 {
                        return Some(pos);
                    }
                }
                _ => pos += 1,
            }
        }
        None
    }

    fn close_dedented_scopes(&mut self, indent: usize) {
        while self.scopes.last().is_some_and(|scope| indent <= scope.header_indent) {
            self.scopes.pop();
        }
    }

    fn skip_multiline_string_literal(&self, line_index: usize) -> Option<usize> {
        let line = self.lines[line_index];
        let mut pos = line.start;
        while pos < line.content_end {
            if self.bytes[pos] == b'#' {
                return None;
            }
            if let Some(lit) = scan_string(self.bytes, pos) {
                if lit.range.end() > line.content_end {
                    return self
                        .line_index_for_offset(lit.range.end())
                        .map(|index| index + 1)
                        .or(Some(self.lines.len()));
                }
                pos = lit.range.end();
            } else {
                pos += 1;
            }
        }
        None
    }

    fn is_inside_class(&self, indent: usize) -> bool {
        self.scopes
            .iter()
            .rev()
            .any(|scope| scope.kind == ScopeKind::Class && indent > scope.header_indent)
    }

    fn current_function_scope_index(&self) -> Option<usize> {
        self.scopes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, scope)| scope.kind == ScopeKind::Function)
            .map(|(index, _)| index)
    }

    fn set_function_return_value(&mut self, item_index: usize) {
        self.summary.items[item_index].has_return_value = true;
    }

    fn set_function_yield(&mut self, item_index: usize) {
        self.summary.items[item_index].has_yield = true;
    }

    fn first_non_ws(&self, start: usize, end: usize) -> usize {
        let mut pos = start;
        while pos < end && matches!(self.bytes[pos], b' ' | b'\t') {
            pos += 1;
        }
        pos
    }

    fn skip_ws(&self, mut pos: usize, end: usize) -> usize {
        while pos < end && matches!(self.bytes[pos], b' ' | b'\t' | b'\n' | b'\r') {
            pos += 1;
        }
        pos
    }

    fn skip_ws_and_comments(&self, mut pos: usize, end: usize) -> usize {
        loop {
            pos = self.skip_ws(pos, end);
            if pos >= end || self.bytes[pos] != b'#' {
                return pos;
            }
            pos = self.line_end_from(pos);
        }
    }

    fn trim_end_ws(&self, start: usize, mut end: usize) -> usize {
        while end > start && matches!(self.bytes[end - 1], b' ' | b'\t') {
            end -= 1;
        }
        end
    }

    fn scan_identifier(&self, start: usize) -> Option<usize> {
        if !self.source.is_char_boundary(start) {
            return None;
        }
        let mut chars = self.source[start..].char_indices();
        let (_, first) = chars.next()?;
        if !is_ident_start_char(first) {
            return None;
        }

        let mut end = start + first.len_utf8();
        for (offset, ch) in chars {
            if !is_ident_continue_char(ch) {
                break;
            }
            end = start + offset + ch.len_utf8();
        }
        Some(end)
    }

    fn at_keyword(&self, pos: usize, keyword: &[u8]) -> bool {
        self.bytes.get(pos..pos + keyword.len()) == Some(keyword)
            && pos
                .checked_sub(1)
                .and_then(|prev| self.bytes.get(prev))
                .is_none_or(|byte| !is_ident_continue_byte(*byte))
            && self
                .bytes
                .get(pos + keyword.len())
                .is_none_or(|byte| !is_ident_continue_byte(*byte))
    }

    fn after_keyword_ws(&self, pos: usize, keyword: &[u8]) -> Option<usize> {
        let after = pos + keyword.len();
        if !self.at_keyword(pos, keyword) || !matches!(self.bytes.get(after), Some(b' ' | b'\t')) {
            return None;
        }
        Some(self.skip_ws(after, self.bytes.len()))
    }

    fn line_end_from(&self, pos: usize) -> usize {
        self.bytes[pos..]
            .iter()
            .position(|byte| *byte == b'\n' || *byte == b'\r')
            .map_or(self.bytes.len(), |offset| pos + offset)
    }

    fn line_index_for_offset(&self, offset: usize) -> Option<usize> {
        self.lines
            .iter()
            .enumerate()
            .find(|(_, line)| line.start <= offset && offset <= line.content_end)
            .map(|(index, _)| index)
    }
}

fn line_ranges(bytes: &[u8]) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut pos = 0;
    while pos < bytes.len() {
        if bytes[pos] == b'\n' {
            let content_end = if pos > start && bytes[pos - 1] == b'\r' {
                pos - 1
            } else {
                pos
            };
            lines.push(Line { start, content_end });
            start = pos + 1;
        }
        pos += 1;
    }
    if start < bytes.len() {
        lines.push(Line {
            start,
            content_end: bytes.len(),
        });
    }
    lines
}

fn line_start_depths(bytes: &[u8], lines: &[Line]) -> Vec<u32> {
    let mut depths = Vec::with_capacity(lines.len());
    let mut depth = 0usize;
    let mut multiline_string_end: Option<usize> = None;

    for line in lines {
        depths.push(depth as u32);
        let mut pos = line.start;

        if let Some(end) = multiline_string_end {
            if line.content_end < end {
                continue;
            }
            if line.start < end {
                pos = end.min(line.content_end);
                multiline_string_end = None;
                if pos >= line.content_end {
                    continue;
                }
            } else {
                multiline_string_end = None;
            }
        }

        while pos < line.content_end {
            if let Some(lit) = scan_string(bytes, pos) {
                if lit.range.end() > line.content_end {
                    multiline_string_end = Some(lit.range.end());
                    break;
                }
                pos = lit.range.end();
                continue;
            }
            match bytes[pos] {
                b'#' => break,
                b'(' | b'[' | b'{' => {
                    depth += 1;
                    pos += 1;
                }
                b')' | b']' | b'}' => {
                    depth = depth.saturating_sub(1);
                    pos += 1;
                }
                _ => pos += 1,
            }
        }
    }

    depths
}

fn is_ident_start_char(ch: char) -> bool {
    ch == '_' || ch.is_alphabetic()
}

fn is_ident_continue_char(ch: char) -> bool {
    is_ident_start_char(ch) || ch.is_ascii_digit()
}

fn is_ident_continue_byte(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric() || byte >= 0x80
}
