//! Templates that send requests. A request block names a method, paths under
//! `{{BaseURL}}` or `{{RootURL}}`, headers and a body; the target the
//! operator chose supplies the scheme, host and port. A path is only ever a
//! path: `{{BaseURL}}@example.net` asks the target for `/@example.net`.

use std::{borrow::Cow, cell::RefCell, collections::HashMap};

use serde_json::Value;

use super::dsl::{self, Allow, CompileError, EvalError, Interp, Val, Vars};
use super::engine::{self, CompiledTemplate, UrlVars};
use super::model::{Classification, ParsedTemplate};

/// Nuclei follows ten redirects unless a template asks for another number;
/// Sniper never follows more.
pub const MAX_REDIRECTS: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Follow {
    None,
    /// `redirects: true`.
    All,
    /// `host-redirects: true`: only while the host stays the one the chain
    /// started on.
    SameHost,
}

#[derive(Clone, Debug)]
struct PathSpec {
    /// `{{RootURL}}`: from the origin, leaving out the target's own path.
    root: bool,
    rest: Interp,
}

/// A template compiled for sending.
#[derive(Clone, Debug)]
pub struct ActiveTemplate {
    pub template: CompiledTemplate,
    method: String,
    paths: Vec<PathSpec>,
    headers: Vec<(String, Interp)>,
    body: Option<Interp>,
    variables: Vec<(String, Interp)>,
    pub stop_at_first_match: bool,
    pub follow: Follow,
    pub max_redirects: usize,
}

/// One request, ready to send to the target it was built for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuiltRequest {
    pub method: String,
    /// Path and query for the request line, starting with `/`.
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

pub fn compile_active(
    template: &ParsedTemplate,
    classification: &Classification,
) -> Result<ActiveTemplate, CompileError> {
    if let Some(first) = classification.active_reasons.first() {
        return Err(CompileError(first.clone()));
    }
    let block = template
        .root
        .get("http")
        .or_else(|| template.root.get("requests"))
        .and_then(Value::as_array)
        .and_then(|blocks| blocks.first())
        .and_then(Value::as_object)
        .ok_or_else(|| engine::reason("http:empty"))?;
    let allow = Allow { random: true };
    // The request is known when its answer is judged, so matchers may read it.
    let operators = engine::compile_operators(block, allow, true)?;
    let method = block
        .get("method")
        .map(engine::text_of)
        .unwrap_or_default()
        .trim()
        .to_ascii_uppercase();
    let method = if method.is_empty() {
        "GET".to_string()
    } else {
        method
    };
    // A tunnel request through a pooled client is not a probe.
    if method == "CONNECT" {
        return Err(engine::reason("req:method"));
    }
    let paths = engine::strings(block, "path")?
        .iter()
        .map(|path| {
            let path = path.trim_start();
            let (root, rest) = match (
                path.strip_prefix("{{BaseURL}}"),
                path.strip_prefix("{{RootURL}}"),
            ) {
                (Some(rest), _) => (false, rest),
                (None, Some(rest)) => (true, rest),
                (None, None) => return Err(engine::reason("req:path_not_base")),
            };
            Ok(PathSpec {
                root,
                rest: Interp::compile(rest, allow)?,
            })
        })
        .collect::<Result<Vec<_>, CompileError>>()?;
    if paths.is_empty() {
        return Err(engine::reason("req:path"));
    }
    let mut headers = Vec::new();
    if let Some(map) = block.get("headers") {
        let map = map
            .as_object()
            .ok_or_else(|| engine::reason("req:headers"))?;
        for (name, value) in map {
            // The classifier allowed only `{{Hostname}}` here, which is the
            // Host every request to the target carries anyway.
            if name.eq_ignore_ascii_case("host") {
                continue;
            }
            if http::HeaderName::from_bytes(name.as_bytes()).is_err() {
                return Err(engine::reason("req:header_name"));
            }
            headers.push((
                name.clone(),
                Interp::compile(&engine::text_of(value), allow)?,
            ));
        }
    }
    let body = block
        .get("body")
        .map(|body| Interp::compile(&engine::text_of(body), allow))
        .transpose()?;
    let variables = match template.root.get("variables") {
        Some(Value::Object(map)) => map
            .iter()
            .map(|(name, value)| {
                Ok((
                    name.clone(),
                    Interp::compile(&engine::text_of(value), allow)?,
                ))
            })
            .collect::<Result<Vec<_>, CompileError>>()?,
        _ => Vec::new(),
    };
    // Nuclei applies host-redirects over redirects when both are set.
    let follow = if engine::flag(block, "host-redirects")? {
        Follow::SameHost
    } else if engine::flag(block, "redirects")? {
        Follow::All
    } else {
        Follow::None
    };
    let max_redirects = match block.get("max-redirects") {
        None | Some(Value::Null) => MAX_REDIRECTS,
        Some(value) => match engine::text_of(value).trim().parse::<usize>() {
            // Zero means Nuclei's default, as there.
            Ok(0) => MAX_REDIRECTS,
            Ok(count) => count.min(MAX_REDIRECTS),
            Err(_) => return Err(engine::reason("field:max-redirects")),
        },
    };
    Ok(ActiveTemplate {
        template: CompiledTemplate {
            info: template.info.clone(),
            passive: None,
            body_sensitive: engine::body_sensitive(&operators),
            operators,
        },
        method,
        paths,
        headers,
        body,
        variables,
        stop_at_first_match: engine::flag(block, "stop-at-first-match")?,
        follow,
        max_redirects,
    })
}

/// What a template's markers read while one run on one target builds its
/// requests: the target, the template's variables, and random strings made
/// on first use and kept for the run, so a matcher looking for a reflected
/// `{{randstr}}` sees the value the request carried.
pub struct RunValues<'a> {
    url: &'a UrlVars,
    values: RefCell<HashMap<String, String>>,
}

impl RunValues<'_> {
    /// The variables and random strings made so far, for the matchers.
    pub fn values(&self) -> HashMap<String, String> {
        self.values.borrow().clone()
    }
}

fn is_random_name(name: &str) -> bool {
    name == "randstr"
        || name
            .strip_prefix("randstr_")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

impl Vars for RunValues<'_> {
    fn get(&self, name: &str) -> Option<Val<'_>> {
        if let Some(value) = self.url.get(name) {
            return Some(Val::Str(Cow::Borrowed(value)));
        }
        if let Some(value) = self.values.borrow().get(name) {
            return Some(Val::Str(Cow::Owned(value.clone())));
        }
        if !is_random_name(name) {
            return None;
        }
        let value = dsl::random_id();
        self.values
            .borrow_mut()
            .insert(name.to_string(), value.clone());
        Some(Val::Str(Cow::Owned(value)))
    }
}

impl ActiveTemplate {
    /// Requests one run sends before any redirect.
    pub fn request_count(&self) -> usize {
        self.paths.len()
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    /// Starts a run on one target: the template's variables are worked out
    /// once, each after the ones it names, whatever order the file lists them.
    pub fn start_run<'a>(&self, url: &'a UrlVars) -> Result<RunValues<'a>, EvalError> {
        let run = RunValues {
            url,
            values: RefCell::new(HashMap::new()),
        };
        let mut pending: Vec<&(String, Interp)> = self.variables.iter().collect();
        while !pending.is_empty() {
            let before = pending.len();
            let mut last_error = EvalError("unresolved");
            pending.retain(|(name, value)| match value.render(&run) {
                Ok(text) => {
                    run.values.borrow_mut().insert(name.clone(), text);
                    false
                }
                Err(error) => {
                    last_error = error;
                    true
                }
            });
            if pending.len() == before {
                return Err(last_error);
            }
        }
        Ok(run)
    }

    /// The `index`th request of a run. `base_path` is the target's own path
    /// without a trailing slash, which `{{BaseURL}}` carries and
    /// `{{RootURL}}` leaves out.
    pub fn build(
        &self,
        index: usize,
        base_path: &str,
        run: &RunValues<'_>,
    ) -> Result<BuiltRequest, EvalError> {
        let spec = self.paths.get(index).ok_or(EvalError("path"))?;
        let rest = spec.rest.render(run)?;
        let prefix = if spec.root { "" } else { base_path };
        let target = request_target(&format!("{prefix}{rest}")).ok_or(EvalError("path"))?;
        let mut headers = Vec::with_capacity(self.headers.len());
        for (name, value) in &self.headers {
            let value = value.render(run)?;
            if http::HeaderValue::from_str(&value).is_err() {
                return Err(EvalError("header"));
            }
            headers.push((name.clone(), value));
        }
        let body = match &self.body {
            Some(body) => body.render(run)?,
            None => String::new(),
        };
        Ok(BuiltRequest {
            method: self.method.clone(),
            target,
            headers,
            body,
        })
    }
}

/// A path and query fit for a request line, as Go sends what Nuclei builds:
/// the fragment left off, and what a request line cannot carry
/// percent-encoded. None when it still is not one.
pub fn request_target(raw: &str) -> Option<String> {
    let raw = raw.split('#').next().unwrap_or("");
    let mut out = String::with_capacity(raw.len() + 1);
    if !raw.starts_with('/') {
        out.push('/');
    }
    for byte in raw.bytes() {
        if byte <= b' ' || byte >= 0x7f || matches!(byte, b'"' | b'<' | b'>' | b'`') {
            out.push_str(&format!("%{byte:02X}"));
        } else {
            out.push(byte as char);
        }
    }
    http::uri::PathAndQuery::try_from(out.as_str()).ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::templates::model::{classify, parse};

    fn compiled(yaml: &str) -> ActiveTemplate {
        let parsed = parse(yaml).unwrap();
        let classification = classify(&parsed);
        compile_active(&parsed, &classification).unwrap()
    }

    fn target(url: &str) -> (UrlVars, String) {
        let url = url::Url::parse(url).unwrap();
        let base_path = url.path().trim_end_matches('/').to_string();
        (UrlVars::of_url(&url, &base_path).unwrap(), base_path)
    }

    #[test]
    fn paths_extend_the_target_and_never_leave_it() {
        let template = compiled(
            "id: example-paths\ninfo: {}\nhttp:\n  - method: post\n    path:\n      - '{{BaseURL}}/.git/config'\n      - '{{RootURL}}/admin?q=<b> c#frag'\n      - '{{BaseURL}}@example.net'\n      - '{{BaseURL}}?next={{BaseURL}}/x'\n    headers:\n      Host: '{{Hostname}}'\n      X-Probe: '{{Host}}:{{Port}}'\n    body: 'a={{Path}}'\n    matchers:\n      - type: status\n        status: [200]\n",
        );
        let (url, base_path) = target("https://example.com:8443/app/");
        assert_eq!(url.hostname, "example.com:8443");
        assert_eq!(url.base_url, "https://example.com:8443/app");
        let run = template.start_run(&url).unwrap();
        let built: Vec<BuiltRequest> = (0..template.request_count())
            .map(|index| template.build(index, &base_path, &run).unwrap())
            .collect();
        assert_eq!(built[0].method, "POST");
        assert_eq!(built[0].target, "/app/.git/config");
        assert_eq!(built[1].target, "/admin?q=%3Cb%3E%20c");
        assert_eq!(built[2].target, "/app@example.net");
        assert_eq!(built[3].target, "/app?next=https://example.com:8443/app/x");
        // Host comes from the target alone.
        assert_eq!(
            built[0].headers,
            [("X-Probe".to_string(), "example.com:8443".to_string())]
        );
        let (root, root_path) = target("http://example.com");
        assert_eq!(root.hostname, "example.com");
        let run = template.start_run(&root).unwrap();
        assert_eq!(
            template.build(0, &root_path, &run).unwrap().target,
            "/.git/config"
        );
        assert_eq!(
            template.build(2, &root_path, &run).unwrap().target,
            "/@example.net"
        );
    }

    #[test]
    fn variables_and_random_values_are_shared_by_the_run() {
        let template = compiled(
            "id: example-vars\ninfo: {}\nvariables:\n  a_digest: '{{md5(z_token)}}'\n  z_token: '{{randstr}}'\nhttp:\n  - method: GET\n    path:\n      - '{{BaseURL}}/?t={{z_token}}&d={{a_digest}}&r={{randstr}}'\n    matchers:\n      - type: word\n        words: ['{{z_token}}']\n",
        );
        let (url, base_path) = target("https://example.com");
        let run = template.start_run(&url).unwrap();
        let built = template.build(0, &base_path, &run).unwrap();
        let values = run.values();
        let token = &values["z_token"];
        assert_eq!(token.len(), 27);
        assert_eq!(values["randstr"], *token);
        assert_eq!(
            built.target,
            format!("/?t={token}&d={}&r={token}", values["a_digest"])
        );
        // Each run makes its own.
        let other = template.start_run(&url).unwrap();
        assert_ne!(other.values()["z_token"], *token);
    }

    #[test]
    fn redirect_settings_follow_nuclei() {
        let none = compiled(
            "id: a\ninfo: {}\nhttp:\n  - path: ['{{BaseURL}}']\n    matchers:\n      - type: status\n        status: [200]\n",
        );
        assert_eq!(none.follow, Follow::None);
        let all = compiled(
            "id: b\ninfo: {}\nhttp:\n  - path: ['{{BaseURL}}']\n    redirects: true\n    max-redirects: 3\n    matchers:\n      - type: status\n        status: [200]\n",
        );
        assert_eq!((all.follow, all.max_redirects), (Follow::All, 3));
        let host = compiled(
            "id: c\ninfo: {}\nhttp:\n  - path: ['{{BaseURL}}']\n    redirects: true\n    host-redirects: true\n    max-redirects: 40\n    stop-at-first-match: true\n    matchers:\n      - type: status\n        status: [200]\n",
        );
        assert_eq!(
            (host.follow, host.max_redirects, host.stop_at_first_match),
            (Follow::SameHost, MAX_REDIRECTS, true)
        );
    }

    #[test]
    fn request_targets_are_what_a_request_line_carries() {
        assert_eq!(request_target("").as_deref(), Some("/"));
        assert_eq!(request_target("?a=1").as_deref(), Some("/?a=1"));
        assert_eq!(
            request_target("/x?q=\"é\"`").as_deref(),
            Some("/x?q=%22%C3%A9%22%60")
        );
        assert_eq!(
            request_target("/..;/a/%2e%2e/{x}").as_deref(),
            Some("/..;/a/%2e%2e/{x}")
        );
    }

    #[test]
    fn an_unbuildable_request_says_so_instead_of_sending_part() {
        let template = compiled(
            "id: example-bad\ninfo: {}\nhttp:\n  - method: GET\n    path: ['{{BaseURL}}/{{base64_decode(\"%%%\")}}']\n    matchers:\n      - type: status\n        status: [200]\n",
        );
        let (url, base_path) = target("https://example.com");
        let run = template.start_run(&url).unwrap();
        assert!(template.build(0, &base_path, &run).is_err());
    }
}
