//! Bounded metadata-only recognition of four declarations in retained Link fields.
//! No I/O, source-context policy, URL resolution, fetching, or Sniper integration.

use url::{Host, Url};

pub const MAX_HEADERS: usize = 128;
pub const MAX_HEADER_BYTES: usize = 32 * 1024;
pub const MAX_LINK_FIELD_BYTES: usize = 4096;
pub const MAX_TARGET_BYTES: usize = 4096;
pub const MAX_CANDIDATES: usize = 128;
pub const MAX_PARAMETERS: usize = 32;
pub const MAX_RELATION_TOKENS: usize = 32;
pub const MAX_OBSERVATIONS: usize = 32;
pub const MAX_TARGET_ORIGIN_BYTES: usize = 268;

#[derive(Clone, Copy, Debug)]
pub struct RetainedHeader<'a> {
    pub name: &'a str,
    pub value: &'a str,
}

/// Half-open UTF-8 byte offsets into the original, unmodified header value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Relation {
    ServiceDoc,
    ServiceDesc,
    DescribedBy,
    Preconnect,
}

impl Relation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ServiceDoc => "service-doc",
            Self::ServiceDesc => "service-desc",
            Self::DescribedBy => "describedby",
            Self::Preconnect => "preconnect",
        }
    }

    pub fn rule_id(self) -> &'static str {
        match self {
            Self::ServiceDoc => "response_link.service_doc.v1",
            Self::ServiceDesc => "response_link.service_desc.v1",
            Self::DescribedBy => "response_link.describedby.v1",
            Self::Preconnect => "response_link.preconnect.v1",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        [
            Self::ServiceDoc,
            Self::ServiceDesc,
            Self::DescribedBy,
            Self::Preconnect,
        ]
        .into_iter()
        .find(|relation| value.eq_ignore_ascii_case(relation.as_str()))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation {
    pub relation: Relation,
    pub target_origin: String,
    pub source: &'static str,
    pub strength: &'static str,
    pub header_name: &'static str,
    pub header_index: usize,
    pub header_value_bytes: usize,
    pub link_value_span: Span,
    pub target_span: Span,
    pub relation_span: Span,
    pub rule_id: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    HeaderCountLimit,
    HeaderByteLimit,
    LinkFieldByteLimit,
    CandidateLimit,
    ParameterLimit,
    RelationTokenLimit,
    ObservationLimit,
    MalformedField,
    UnsupportedTarget,
    AnchorPresent,
    MissingRelation,
    UnsupportedRelationSyntax,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Coverage {
    CompleteWithinFixedGrammar,
    Partial,
}

/// Counts describe inspected input only; no estimate of skipped tails is made.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Counts {
    pub headers_inspected: usize,
    pub header_bytes_admitted: usize,
    pub link_fields_inspected: usize,
    pub candidates_inspected: usize,
    pub members_framed: usize,
    pub parameters_inspected: usize,
    pub relation_tokens_inspected: usize,
    pub unknown_relation_tokens_inspected: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Report {
    pub observations: Vec<Observation>,
    pub coverage: Coverage,
    pub reasons: Vec<Reason>,
    pub counts: Counts,
}

impl Default for Report {
    fn default() -> Self {
        Self {
            observations: Vec::new(),
            coverage: Coverage::CompleteWithinFixedGrammar,
            reasons: Vec::new(),
            counts: Counts::default(),
        }
    }
}

impl Report {
    fn note(&mut self, reason: Reason) {
        self.coverage = Coverage::Partial;
        if !self.reasons.contains(&reason) {
            self.reasons.push(reason);
        }
    }
}

/// Input must be retained response fields in original order. The caller owns
/// response/method/context eligibility; this function deliberately cannot infer it.
pub fn inspect(headers: &[RetainedHeader<'_>]) -> Report {
    let mut report = Report::default();
    for (header_index, header) in headers.iter().enumerate() {
        if header_index == MAX_HEADERS {
            report.note(Reason::HeaderCountLimit);
            break;
        }
        let remaining = MAX_HEADER_BYTES - report.counts.header_bytes_admitted;
        if header.name.len() > remaining || header.value.len() > remaining - header.name.len() {
            report.note(Reason::HeaderByteLimit);
            break;
        }
        report.counts.headers_inspected += 1;
        report.counts.header_bytes_admitted += header.name.len() + header.value.len();
        if !header.name.eq_ignore_ascii_case("link") {
            continue;
        }
        report.counts.link_fields_inspected += 1;
        if header.value.len() > MAX_LINK_FIELD_BYTES {
            report.note(Reason::LinkFieldByteLimit);
            continue;
        }
        if !inspect_field(header.value, header_index, &mut report) {
            break;
        }
    }
    report
}

// false means the row-level candidate budget was encountered; other failures
// stop only the current field, whose framing cannot safely be recovered.
fn inspect_field(value: &str, header_index: usize, report: &mut Report) -> bool {
    let bytes = value.as_bytes();
    let mut pos = 0;
    loop {
        skip_ows(bytes, &mut pos);
        // Empty list elements have no declaration and consume no candidate slot.
        if bytes.get(pos) == Some(&b',') {
            pos += 1;
            continue;
        }
        if pos == bytes.len() {
            return true;
        }
        if bytes[pos] != b'<' {
            report.note(Reason::MalformedField);
            return true;
        }
        if report.counts.candidates_inspected == MAX_CANDIDATES {
            report.note(Reason::CandidateLimit);
            return false;
        }
        report.counts.candidates_inspected += 1;
        let member = match frame_member(bytes, &mut pos, &mut report.counts) {
            Ok(member) => member,
            Err(reason) => {
                report.note(reason);
                return true;
            }
        };
        report.counts.members_framed += 1;
        inspect_member(value, header_index, member, report);
        // frame_member stops before a separating comma or at the field end.
    }
}

#[derive(Clone, Copy)]
struct ParameterValue {
    span: Span,
    escaped: bool,
}

struct Member {
    span: Span,
    target: Span,
    first_rel: Option<ParameterValue>,
    rel_seen: bool,
    anchor_seen: bool,
}

fn frame_member(bytes: &[u8], pos: &mut usize, counts: &mut Counts) -> Result<Member, Reason> {
    let start = *pos;
    *pos += 1;
    let target_start = *pos;
    while let Some(&byte) = bytes.get(*pos) {
        if byte == b'>' {
            break;
        }
        if byte == b'<' {
            return Err(Reason::MalformedField);
        }
        *pos += 1;
    }
    if bytes.get(*pos) != Some(&b'>') {
        return Err(Reason::MalformedField);
    }
    let target = Span {
        start: target_start,
        end: *pos,
    };
    *pos += 1;
    let mut end = *pos;
    let mut first_rel = None;
    let mut rel_seen = false;
    let mut anchor_seen = false;
    let mut parameters = 0;
    loop {
        skip_ows(bytes, pos);
        match bytes.get(*pos) {
            None | Some(b',') => break,
            Some(b';') => *pos += 1,
            _ => return Err(Reason::MalformedField),
        }
        skip_ows(bytes, pos);
        if !bytes.get(*pos).is_some_and(|b| is_token(*b)) {
            return Err(Reason::MalformedField);
        }
        if parameters == MAX_PARAMETERS {
            return Err(Reason::ParameterLimit);
        }
        parameters += 1;
        counts.parameters_inspected += 1;
        let name_start = *pos;
        while bytes.get(*pos).is_some_and(|b| is_token(*b)) {
            *pos += 1;
        }
        let name = &bytes[name_start..*pos];
        // The name alone is enough to reject even an empty or valueless anchor.
        anchor_seen |= name.eq_ignore_ascii_case(b"anchor");
        let is_first_rel = !rel_seen && name.eq_ignore_ascii_case(b"rel");
        rel_seen |= is_first_rel;
        end = *pos;
        skip_ows(bytes, pos);
        let parameter_value = if bytes.get(*pos) == Some(&b'=') {
            *pos += 1;
            skip_ows(bytes, pos);
            let parsed = frame_parameter_value(bytes, pos)?;
            end = *pos;
            Some(parsed)
        } else {
            None
        };
        if is_first_rel {
            first_rel = parameter_value;
        }
    }
    Ok(Member {
        span: Span { start, end },
        target,
        first_rel,
        rel_seen,
        anchor_seen,
    })
}

fn frame_parameter_value(bytes: &[u8], pos: &mut usize) -> Result<ParameterValue, Reason> {
    if bytes.get(*pos) != Some(&b'"') {
        let start = *pos;
        while bytes.get(*pos).is_some_and(|b| is_token(*b)) {
            *pos += 1;
        }
        if *pos == start {
            return Err(Reason::MalformedField);
        }
        return Ok(ParameterValue {
            span: Span { start, end: *pos },
            escaped: false,
        });
    }
    *pos += 1;
    let start = *pos;
    let mut escaped = false;
    while let Some(&byte) = bytes.get(*pos) {
        match byte {
            b'"' => {
                let end = *pos;
                *pos += 1;
                return Ok(ParameterValue {
                    span: Span { start, end },
                    escaped,
                });
            }
            b'\\' => {
                escaped = true;
                *pos += 1;
                // Quoted-pair accepts HTAB, SP, VCHAR and obs-text. Do not
                // unescape or export it: only its framing matters here.
                if !bytes
                    .get(*pos)
                    .is_some_and(|b| *b == b'\t' || *b >= 0x20 && *b != 0x7f)
                {
                    return Err(Reason::MalformedField);
                }
                *pos += 1;
            }
            b'\t' | 0x20..=0x21 | 0x23..=0x5b | 0x5d..=0xff if byte != 0x7f => *pos += 1,
            _ => return Err(Reason::MalformedField),
        }
    }
    Err(Reason::MalformedField)
}

fn inspect_member(value: &str, header_index: usize, member: Member, report: &mut Report) {
    // Nothing is emitted until all parameters have been framed, including a late anchor.
    if member.anchor_seen {
        report.note(Reason::AnchorPresent);
        return;
    }
    if !member.rel_seen {
        report.note(Reason::MissingRelation);
        return;
    }
    let Some(rel) = member.first_rel else {
        report.note(Reason::UnsupportedRelationSyntax);
        return;
    };
    if rel.escaped {
        report.note(Reason::UnsupportedRelationSyntax);
        return;
    }
    let Some(relations) = inspect_relations(value, rel.span, report) else {
        return;
    };
    let Some(target_origin) = target_origin(&value[member.target.start..member.target.end]) else {
        report.note(Reason::UnsupportedTarget);
        return;
    };
    for (relation, relation_span) in relations {
        if report.observations.len() == MAX_OBSERVATIONS {
            report.note(Reason::ObservationLimit);
            // Continue bounded framing after the output fills. A cap reason
            // is added only for a verified observation actually omitted.
            break;
        }
        report.observations.push(Observation {
            relation,
            target_origin: target_origin.clone(),
            source: "response_link_header",
            strength: "literal_declaration",
            header_name: "link",
            header_index,
            header_value_bytes: value.len(),
            link_value_span: member.span,
            target_span: member.target,
            relation_span,
            rule_id: relation.rule_id(),
        });
    }
}

fn inspect_relations(
    value: &str,
    span: Span,
    report: &mut Report,
) -> Option<Vec<(Relation, Span)>> {
    let bytes = value.as_bytes();
    let mut pos = span.start;
    let mut inspected = 0;
    let mut found = Vec::new();
    if pos == span.end || bytes[pos] == b' ' || bytes[span.end - 1] == b' ' {
        report.note(Reason::UnsupportedRelationSyntax);
        return None;
    }
    while pos < span.end {
        if inspected == MAX_RELATION_TOKENS {
            report.note(Reason::RelationTokenLimit);
            return None;
        }
        let start = pos;
        while pos < span.end && bytes[pos] != b' ' {
            pos += 1;
        }
        inspected += 1;
        report.counts.relation_tokens_inspected += 1;
        let token = &bytes[start..pos];
        // URI-valued extension relations and escaped rel spellings are outside
        // this recognizer. An unsupported first value suppresses the member.
        if !token[0].is_ascii_alphabetic()
            || !token
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'-'))
        {
            report.note(Reason::UnsupportedRelationSyntax);
            return None;
        }
        if let Some(relation) = Relation::parse(&value[start..pos]) {
            found.push((relation, Span { start, end: pos }));
        } else {
            report.counts.unknown_relation_tokens_inspected += 1;
        }
        while pos < span.end && bytes[pos] == b' ' {
            pos += 1;
        }
    }
    Some(found)
}

fn skip_ows(bytes: &[u8], pos: &mut usize) {
    while bytes.get(*pos).is_some_and(|b| matches!(*b, b' ' | b'\t')) {
        *pos += 1;
    }
}

fn is_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

/// Validate the original target before passing ONLY its authority to Url. No
/// whole-target repair, relative resolution, percent decoding or path export.
fn target_origin(target: &str) -> Option<String> {
    if target.len() > MAX_TARGET_BYTES || !target.is_ascii() {
        return None;
    }
    let (scheme, rest) = target.split_once("://")?;
    let scheme = if scheme.eq_ignore_ascii_case("https") {
        "https"
    } else if scheme.eq_ignore_ascii_case("http") {
        "http"
    } else {
        return None;
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let (host, port) = if authority.starts_with('[') {
        let close = authority.find(']')?;
        let host = &authority[..=close];
        host[1..host.len() - 1].parse::<std::net::Ipv6Addr>().ok()?;
        let suffix = &authority[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else {
            Some(suffix.strip_prefix(':')?)
        };
        (host, port)
    } else {
        let (host, port) = authority
            .split_once(':')
            .map_or((authority, None), |(h, p)| (h, Some(p)));
        let dns = host.strip_suffix('.').unwrap_or(host);
        if dns.is_empty() || dns.len() > 253 || host.len() > 254 {
            return None;
        }
        for label in dns.split('.') {
            if label.is_empty()
                || label.len() > 63
                || !label.as_bytes()[0].is_ascii_alphanumeric()
                || !label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                || !label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            {
                return None;
            }
        }
        (host, port)
    };
    if let Some(port) = port {
        if port.is_empty()
            || port.len() > 5
            || !port.bytes().all(|b| b.is_ascii_digit())
            || port.len() > 1 && port.starts_with('0')
        {
            return None;
        }
        port.parse::<u16>().ok()?;
    }
    if !valid_suffix(&rest[authority_end..]) {
        return None;
    }
    let url = Url::parse(&format!("{scheme}://{authority}/")).ok()?;
    match url.host()? {
        // Reject WHATWG numeric aliases, trailing-dot numeric hosts, decoded
        // hostnames, or other host repairs; allow case and bracketed IPv6 only.
        Host::Domain(parsed) if !parsed.eq_ignore_ascii_case(host) => return None,
        Host::Ipv4(parsed) if parsed.to_string() != host => return None,
        Host::Ipv6(_) if !host.starts_with('[') => return None,
        _ => {}
    }
    let origin = url.origin().ascii_serialization();
    (origin.len() <= MAX_TARGET_ORIGIN_BYTES).then_some(origin)
}

fn valid_suffix(suffix: &str) -> bool {
    let bytes = suffix.as_bytes();
    let mut pos = 0;
    let mut in_fragment = false;
    while let Some(&byte) = bytes.get(pos) {
        match byte {
            b'%' => {
                if !bytes.get(pos + 1).is_some_and(u8::is_ascii_hexdigit)
                    || !bytes.get(pos + 2).is_some_and(u8::is_ascii_hexdigit)
                {
                    return false;
                }
                pos += 3;
                continue;
            }
            // '?' is a path delimiter or literal query/fragment data.
            b'?' => {}
            b'#' if !in_fragment => in_fragment = true,
            b'#' => return false,
            b'/' => {}
            b':' | b'@' | b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';'
            | b'=' | b'-' | b'.' | b'_' | b'~' => {}
            b if b.is_ascii_alphanumeric() => {}
            _ => return false,
        }
        pos += 1;
    }
    true
}

#[cfg(test)]
mod tests;
