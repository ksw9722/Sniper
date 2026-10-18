//! Deliberately incomplete literal HTML generator declaration recognizer.
//! The caller establishes eligibility and source lifetime; this parser accepts
//! supplied UTF-8 text and never acquires, evaluates, logs, or persists it.

pub const MAX_INPUT_BYTES: usize = 256 * 1024;
pub const MAX_TAG_CANDIDATES: usize = 4096;
pub const MAX_TAG_BYTES: usize = 4096;
pub const MAX_ATTRIBUTES: usize = 32;
pub const MAX_OBSERVATIONS: usize = 8;
pub const MAX_VERSION_BYTES: usize = 32;
pub const MAX_VERSION_SEPARATOR_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Product {
    WordPress,
    Drupal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionStatus {
    Reported,
    NotReported,
    UnsupportedFormat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffsetBasis {
    RetainedUtf8PreviewBytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub product: Product,
    pub reported_version: Option<String>,
    pub version_status: VersionStatus,
    pub tag_span: ByteSpan,
    pub content_span: ByteSpan,
    pub product_span: ByteSpan,
    pub version_span: Option<ByteSpan>,
    pub offset_basis: OffsetBasis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    EndOfPreview,
    InputLimit,
    TagCandidateLimit,
    TagByteLimit,
    AttributeLimit,
    ObservationLimit,
    UnsupportedDocumentContext,
    UnsupportedHeadContext,
    UnsupportedDoctype,
    UnsupportedComment,
    UnsupportedScriptEscape,
    UnsupportedEntity,
    UnsupportedSelfClosing,
    MalformedTag,
    DuplicateAttribute,
    NullByte,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coverage {
    /// Only the explicit head was inspected. The document tail was not parsed.
    ExplicitHeadClosed,
    Partial {
        reason: StopReason,
        /// Raw byte cursor; a byte-budget stop can split a UTF-8 character.
        /// This is not a string-slicing boundary or an observation span.
        at_byte: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inspection {
    pub coverage: Coverage,
    pub observations: Vec<Observation>,
    pub input_bytes_available: usize,
    /// Bounded UTF-8 prefix offered to the recognizer, not bytes fully parsed.
    pub input_bytes_considered: usize,
    pub input_limit_exceeded: bool,
    /// Includes start/end tags and doctype, but not comments or raw-text decoys.
    pub tag_candidates_inspected: usize,
    /// True only after an actual next tag candidate was encountered.
    pub additional_tag_candidate_encountered: bool,
    /// True only after an actual ninth supported declaration was recognized.
    pub additional_observation_encountered: bool,
    /// Exact count among inspected, quoted name=generator declarations only.
    pub unknown_product_declarations: usize,
}

#[derive(Clone, Copy)]
struct Stop {
    reason: StopReason,
    at: usize,
}

type ParseResult<T> = Result<T, Stop>;

#[derive(Clone, Copy)]
struct Attribute {
    name: ByteSpan,
    value: Option<ByteSpan>,
}

struct Tag {
    span: ByteSpan,
    name: ByteSpan,
    end_tag: bool,
    self_closing: bool,
    attrs: Vec<Attribute>,
}

struct Scanner<'a> {
    input: &'a [u8],
    position: usize,
    result: Inspection,
}

fn space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0x0c)
}

fn name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || matches!(b, b'_' | b':')
}

fn name_byte(b: u8) -> bool {
    name_start(b) || b.is_ascii_digit() || matches!(b, b'-' | b'.')
}

/// The caller must establish media type, decoding, selection, and source
/// lifetime separately. This prototype makes no source-eligibility decision.
pub fn inspect_html_generator(preview: &str) -> Inspection {
    let mut end = preview.len().min(MAX_INPUT_BYTES);
    while !preview.is_char_boundary(end) {
        end -= 1;
    }
    let mut scanner = Scanner {
        input: &preview.as_bytes()[..end],
        position: 0,
        result: Inspection {
            coverage: Coverage::Partial {
                reason: StopReason::EndOfPreview,
                at_byte: 0,
            },
            observations: Vec::with_capacity(MAX_OBSERVATIONS),
            input_bytes_available: preview.len(),
            input_bytes_considered: end,
            input_limit_exceeded: preview.len() > MAX_INPUT_BYTES,
            tag_candidates_inspected: 0,
            additional_tag_candidate_encountered: false,
            additional_observation_encountered: false,
            unknown_product_declarations: 0,
        },
    };
    scanner.result.coverage = match scanner.scan() {
        Ok(()) => Coverage::ExplicitHeadClosed,
        Err(stop) => Coverage::Partial {
            reason: stop.reason,
            at_byte: stop.at,
        },
    };
    scanner.result
}

impl Scanner<'_> {
    fn stop<T>(&self, reason: StopReason, at: usize) -> ParseResult<T> {
        Err(Stop { reason, at })
    }

    fn eof<T>(&self) -> ParseResult<T> {
        self.stop(
            if self.result.input_limit_exceeded {
                StopReason::InputLimit
            } else {
                StopReason::EndOfPreview
            },
            self.input.len(),
        )
    }

    fn eq(&self, span: ByteSpan, literal: &[u8]) -> bool {
        self.input[span.start..span.end].eq_ignore_ascii_case(literal)
    }

    fn starts(&self, at: usize, literal: &[u8]) -> bool {
        self.input
            .get(at..at.saturating_add(literal.len()))
            .is_some_and(|bytes| bytes.eq_ignore_ascii_case(literal))
    }

    fn budget_candidate(&mut self, at: usize) -> ParseResult<()> {
        if self.result.tag_candidates_inspected == MAX_TAG_CANDIDATES {
            self.result.additional_tag_candidate_encountered = true;
            return self.stop(StopReason::TagCandidateLimit, at);
        }
        self.result.tag_candidates_inspected += 1;
        Ok(())
    }

    /// Checks the per-tag budget before accessing each byte, including delimiters.
    fn tag_byte(&self, start: usize, at: usize) -> ParseResult<u8> {
        if at >= self.input.len() {
            return self.eof();
        }
        if at - start >= MAX_TAG_BYTES {
            return self.stop(StopReason::TagByteLimit, at);
        }
        if self.input[at] == 0 {
            return self.stop(StopReason::NullByte, at);
        }
        Ok(self.input[at])
    }

    fn tag(&mut self) -> ParseResult<Tag> {
        let start = self.position;
        let mut p = start + 1;
        let end_tag = self.tag_byte(start, p)? == b'/';
        if end_tag {
            p += 1;
        }
        if !self.tag_byte(start, p)?.is_ascii_alphabetic() {
            return self.stop(StopReason::MalformedTag, p);
        }
        self.budget_candidate(start)?;
        let name_begin = p;
        while name_byte(self.tag_byte(start, p)?) {
            p += 1;
        }
        let name = ByteSpan {
            start: name_begin,
            end: p,
        };
        let mut attrs: Vec<Attribute> = Vec::with_capacity(MAX_ATTRIBUTES);
        loop {
            let mut b = self.tag_byte(start, p)?;
            let had_space = space(b);
            while space(b) {
                p += 1;
                b = self.tag_byte(start, p)?;
            }
            let self_closing = b == b'/';
            if self_closing {
                p += 1;
                b = self.tag_byte(start, p)?;
            }
            if b == b'>' {
                if end_tag && (self_closing || !attrs.is_empty()) {
                    return self.stop(StopReason::MalformedTag, p);
                }
                self.position = p + 1;
                return Ok(Tag {
                    span: ByteSpan { start, end: p + 1 },
                    name,
                    end_tag,
                    self_closing,
                    attrs,
                });
            }
            if self_closing || end_tag || !had_space || !name_start(b) {
                return self.stop(StopReason::MalformedTag, p);
            }
            if attrs.len() == MAX_ATTRIBUTES {
                return self.stop(StopReason::AttributeLimit, p);
            }
            let attr_start = p;
            while name_byte(self.tag_byte(start, p)?) {
                p += 1;
            }
            let attr_name = ByteSpan {
                start: attr_start,
                end: p,
            };
            if attrs.iter().any(|a| {
                self.input[a.name.start..a.name.end]
                    .eq_ignore_ascii_case(&self.input[attr_name.start..attr_name.end])
            }) {
                return self.stop(StopReason::DuplicateAttribute, attr_start);
            }
            let after_name = p;
            while space(self.tag_byte(start, p)?) {
                p += 1;
            }
            if self.tag_byte(start, p)? != b'=' {
                attrs.push(Attribute {
                    name: attr_name,
                    value: None,
                });
                // Let the outer loop validate the delimiter/next attribute.
                // Keep its whitespace so adjacent names cannot be accepted.
                p = after_name;
                continue;
            }
            p += 1;
            while space(self.tag_byte(start, p)?) {
                p += 1;
            }
            let quote = self.tag_byte(start, p)?;
            if !matches!(quote, b'\'' | b'"') {
                return self.stop(StopReason::MalformedTag, p);
            }
            p += 1;
            let value_start = p;
            loop {
                let b = self.tag_byte(start, p)?;
                if b == quote {
                    break;
                }
                // Character references cannot end a quoted HTML attribute.
                // Keep unrelated values opaque; semantic fields are checked
                // only after the complete tag and duplicate names are known.
                p += 1;
            }
            attrs.push(Attribute {
                name: attr_name,
                value: Some(ByteSpan {
                    start: value_start,
                    end: p,
                }),
            });
            p += 1;
        }
    }

    fn comment(&mut self) -> ParseResult<()> {
        let mut p = self.position + 4;
        if self.starts(p, b">") || self.starts(p, b"->") {
            return self.stop(StopReason::UnsupportedComment, p);
        }
        while p < self.input.len() {
            if self.input[p] == 0 {
                return self.stop(StopReason::NullByte, p);
            }
            if self.starts(p, b"-->") {
                self.position = p + 3;
                return Ok(());
            }
            // Other double-hyphen forms have error-recovery semantics outside
            // this subset. Never scan through them looking for a later closer.
            if self.starts(p, b"--") {
                return self.stop(StopReason::UnsupportedComment, p);
            }
            p += 1;
        }
        self.eof()
    }

    fn trivia(&mut self) -> ParseResult<()> {
        loop {
            while self.position < self.input.len() && space(self.input[self.position]) {
                self.position += 1;
            }
            if self.position == self.input.len() {
                return self.eof();
            }
            if self.input[self.position] == 0 {
                return self.stop(StopReason::NullByte, self.position);
            }
            if self.starts(self.position, b"<!--") {
                self.comment()?;
            } else {
                return Ok(());
            }
        }
    }

    fn doctype(&mut self) -> ParseResult<()> {
        let start = self.position;
        self.budget_candidate(start)?;
        let mut p = start + b"<!doctype".len();
        if !space(self.tag_byte(start, p)?) {
            return self.stop(StopReason::UnsupportedDoctype, p);
        }
        while space(self.tag_byte(start, p)?) {
            p += 1;
        }
        for expected in b"html" {
            if !self.tag_byte(start, p)?.eq_ignore_ascii_case(expected) {
                return self.stop(StopReason::UnsupportedDoctype, p);
            }
            p += 1;
        }
        while space(self.tag_byte(start, p)?) {
            p += 1;
        }
        if self.tag_byte(start, p)? != b'>' {
            return self.stop(StopReason::UnsupportedDoctype, p);
        }
        self.position = p + 1;
        Ok(())
    }

    fn required_start(&mut self, name: &[u8]) -> ParseResult<()> {
        self.trivia()?;
        if !self.starts(self.position, b"<") {
            return self.stop(StopReason::UnsupportedDocumentContext, self.position);
        }
        let tag = self.tag()?;
        if tag.end_tag || !self.eq(tag.name, name) {
            return self.stop(StopReason::UnsupportedDocumentContext, tag.span.start);
        }
        if tag.self_closing {
            return self.stop(StopReason::UnsupportedSelfClosing, tag.span.start);
        }
        Ok(())
    }

    fn raw_text(&mut self, name: ByteSpan, script: bool) -> ParseResult<()> {
        let literal = &self.input[name.start..name.end];
        let mut p = self.position;
        while p < self.input.len() {
            if self.input[p] == 0 {
                return self.stop(StopReason::NullByte, p);
            }
            if script && self.starts(p, b"<!--") {
                return self.stop(StopReason::UnsupportedScriptEscape, p);
            }
            // In title RCDATA, character references produce text, not new
            // markup. Skipping their original spelling cannot introduce a
            // closing tag; only a literal appropriate end tag changes context.
            if self.starts(p, b"</") && self.starts(p + 2, literal) {
                let after = p + 2 + literal.len();
                if self
                    .input
                    .get(after)
                    .is_some_and(|b| space(*b) || matches!(*b, b'/' | b'>'))
                {
                    self.position = p;
                    let closing = self.tag()?;
                    if !closing.end_tag || !self.eq(closing.name, literal) {
                        return self.stop(StopReason::MalformedTag, p);
                    }
                    return Ok(());
                }
            }
            p += 1;
        }
        self.eof()
    }

    fn meta(&mut self, tag: Tag) -> ParseResult<()> {
        let name = self.quoted_meta_value(&tag, b"name")?;
        let content = self.quoted_meta_value(&tag, b"content")?;
        let Some(name) = name else {
            return Ok(());
        };
        self.reject_entities(name)?;
        if !self.eq(name, b"generator") {
            return Ok(());
        }
        let Some(content) = content else {
            return Ok(());
        };
        self.reject_entities(content)?;
        let value = &self.input[content.start..content.end];
        let product_len = value.iter().position(|b| space(*b)).unwrap_or(value.len());
        let product = match &value[..product_len] {
            p if p.eq_ignore_ascii_case(b"WordPress") => Product::WordPress,
            p if p.eq_ignore_ascii_case(b"Drupal") => Product::Drupal,
            _ => {
                self.result.unknown_product_declarations += 1;
                return Ok(());
            }
        };
        if self.result.observations.len() == MAX_OBSERVATIONS {
            self.result.additional_observation_encountered = true;
            return self.stop(StopReason::ObservationLimit, tag.span.start);
        }
        let suffix = &value[product_len..];
        let separator_bytes = suffix
            .iter()
            .take(MAX_VERSION_SEPARATOR_BYTES + 1)
            .take_while(|b| space(**b))
            .count();
        let (reported_version, version_status, version_span) = if suffix.is_empty() {
            (None, VersionStatus::NotReported, None)
        } else if (1..=MAX_VERSION_SEPARATOR_BYTES).contains(&separator_bytes)
            && valid_version(&suffix[separator_bytes..])
        {
            // valid_version only accepts bounded ASCII, so conversion cannot
            // copy rejected data or change its original byte coordinates.
            (
                Some(
                    String::from_utf8(suffix[separator_bytes..].to_vec()).expect("validated ASCII"),
                ),
                VersionStatus::Reported,
                Some(ByteSpan {
                    start: content.start + product_len + separator_bytes,
                    end: content.end,
                }),
            )
        } else {
            (None, VersionStatus::UnsupportedFormat, None)
        };
        self.result.observations.push(Observation {
            product,
            reported_version,
            version_status,
            tag_span: tag.span,
            content_span: content,
            product_span: ByteSpan {
                start: content.start,
                end: content.start + product_len,
            },
            version_span,
            offset_basis: OffsetBasis::RetainedUtf8PreviewBytes,
        });
        Ok(())
    }

    fn quoted_meta_value(&self, tag: &Tag, name: &[u8]) -> ParseResult<Option<ByteSpan>> {
        match tag.attrs.iter().find(|a| self.eq(a.name, name)) {
            Some(Attribute { value: None, name }) => {
                self.stop(StopReason::MalformedTag, name.start)
            }
            Some(attribute) => Ok(attribute.value),
            None => Ok(None),
        }
    }

    fn reject_entities(&self, span: ByteSpan) -> ParseResult<()> {
        if let Some(index) = self.input[span.start..span.end]
            .iter()
            .position(|b| *b == b'&')
        {
            return self.stop(StopReason::UnsupportedEntity, span.start + index);
        }
        Ok(())
    }

    fn scan(&mut self) -> ParseResult<()> {
        if self.input.starts_with(&[0xef, 0xbb, 0xbf]) {
            self.position = 3;
        }
        self.trivia()?;
        if self.starts(self.position, b"<!doctype") {
            self.doctype()?;
        }
        self.required_start(b"html")?;
        self.required_start(b"head")?;
        loop {
            self.trivia()?;
            if !self.starts(self.position, b"<") {
                return self.stop(StopReason::UnsupportedHeadContext, self.position);
            }
            let tag = self.tag()?;
            if tag.end_tag && self.eq(tag.name, b"head") {
                return Ok(());
            }
            if tag.end_tag {
                return self.stop(StopReason::UnsupportedHeadContext, tag.span.start);
            }
            if self.eq(tag.name, b"meta") {
                self.meta(tag)?;
            } else if self.eq(tag.name, b"base") || self.eq(tag.name, b"link") {
                // Complete inert void tags cannot introduce a descendant context.
            } else if self.eq(tag.name, b"script")
                || self.eq(tag.name, b"style")
                || self.eq(tag.name, b"title")
            {
                if tag.self_closing {
                    return self.stop(StopReason::UnsupportedSelfClosing, tag.span.start);
                }
                self.raw_text(tag.name, self.eq(tag.name, b"script"))?;
            } else {
                // Includes template, foreign content, noscript, and elements
                // that would implicitly close the head in a browser parser.
                return self.stop(StopReason::UnsupportedHeadContext, tag.span.start);
            }
        }
    }
}

fn valid_version(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes.len() <= MAX_VERSION_BYTES
        && bytes
            .split(|b| *b == b'.')
            .all(|part| !part.is_empty() && part.iter().all(u8::is_ascii_digit))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
