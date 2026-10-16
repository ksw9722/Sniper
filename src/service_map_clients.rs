//! Which client sent a request: a browser, an app's WebView, an app's HTTP
//! stack, a tool, a library or a crawler, read from its User-Agent and, for
//! Chromium browsers, the Client Hints naming their brand and platform.
//!
//! Only the fixed names below leave this module; the headers themselves never
//! reach the map. A User-Agent no rule names is "Other".
use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::{LazyLock, Mutex},
};

use regex::{Regex, RegexSet};

pub const BROWSER: &str = "browser";
pub const WEBVIEW: &str = "webview";
pub const APP: &str = "app";
pub const TOOL: &str = "tool";
pub const LIBRARY: &str = "library";
pub const BOT: &str = "bot";
pub const UNKNOWN: &str = "unknown";
pub const KINDS: [&str; 7] = [BROWSER, WEBVIEW, APP, TOOL, LIBRARY, BOT, UNKNOWN];
/// Every operating system a client may be given.
pub const SYSTEMS: [&str; 6] = ["Windows", "macOS", "iOS", "Android", "ChromeOS", "Linux"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Client {
    pub kind: &'static str,
    pub name: &'static str,
    /// The major version, when the client names one.
    pub version: Option<u32>,
    pub os: Option<&'static str>,
}

// The first rule that matches names the client, so each sits before the
// broader ones that would also match it: crawlers before the browsers they
// claim to be, tools before the libraries they are built on, any other
// crawler only after the tools, libraries and apps, whose own URLs may say
// "bot", and before the browsers it may claim to be, every Chromium
// browser before Chrome, and Safari before an iOS app's WebView, which sends
// Safari's User-Agent without its `Safari/` token. A rule's first capture that
// matched is the major version.
const RULES: &[(&str, &str, &str)] = &[
    (r"(?i)\bGooglebot(?:-[a-z]+)?/(\d+)", BOT, "Googlebot"),
    (r"(?i)\bbingbot/(\d+)", BOT, "Bingbot"),
    (r"\bYeti/(\d+)", BOT, "Yeti"),
    (r"\bApplebot/(\d+)", BOT, "Applebot"),
    (r"\bDuckDuckBot(?:-[A-Za-z]+)?/(\d+)", BOT, "DuckDuckBot"),
    (r"(?i)\bBaiduspider(?:-[a-z]+)?/(\d+)", BOT, "Baiduspider"),
    (r"\bYandexBot/(\d+)", BOT, "YandexBot"),
    (r"\bfacebookexternalhit/(\d+)", BOT, "Facebook link preview"),
    (r"\bTwitterbot/(\d+)", BOT, "Twitterbot"),
    (r"\bSlackbot\b", BOT, "Slackbot"),
    (r"\bDiscordbot/(\d+)", BOT, "Discordbot"),
    (r"\bLinkedInBot/(\d+)", BOT, "LinkedInBot"),
    (r"\bGPTBot/(\d+)", BOT, "GPTBot"),
    (r"\bClaudeBot/(\d+)", BOT, "ClaudeBot"),
    (r"(?i)\bsqlmap/(\d+)", TOOL, "sqlmap"),
    (r"Nmap Scripting Engine", TOOL, "Nmap"),
    (r"\bNikto\b", TOOL, "Nikto"),
    (r"\bWPScan v?(\d+)", TOOL, "WPScan"),
    (r"\bHeadlessChrome/(\d+)", TOOL, "Headless Chrome"),
    (r"^curl/(\d+)", TOOL, "curl"),
    (r"(?i)^Wget/(\d+)", TOOL, "Wget"),
    (r"^HTTPie/(\d+)", TOOL, "HTTPie"),
    (r"\bPostmanRuntime/(\d+)", TOOL, "Postman"),
    (r"(?i)^insomnia/(\d+)", TOOL, "Insomnia"),
    (r"\bpython-requests/(\d+)", LIBRARY, "python-requests"),
    (r"\bpython-httpx/(\d+)", LIBRARY, "HTTPX"),
    (r"\baiohttp/(\d+)", LIBRARY, "aiohttp"),
    (r"\bPython-urllib/(\d+)", LIBRARY, "Python urllib"),
    (r"^Go-http-client/(\d+)", LIBRARY, "Go net/http"),
    (r"\bApache-HttpClient/(\d+)", LIBRARY, "Apache HttpClient"),
    (r"^Java/(\d+)", LIBRARY, "Java"),
    (r"^axios/(\d+)", LIBRARY, "axios"),
    (r"^node-fetch/(\d+)|^node\b|^undici\b", LIBRARY, "Node.js"),
    (r"(?i)\bokhttp/(\d+)", APP, "OkHttp"),
    (r"\bDalvik/(\d+)", APP, "Android app"),
    (r"\bAlamofire/(\d+)", APP, "Alamofire"),
    (r"\bCFNetwork/\d+.*\bDarwin/", APP, "Apple app"),
    (
        r"(?i)\b(?:bot|crawler|spider)\b|[a-z](?:bot|crawler|spider)/",
        BOT,
        "Other crawler",
    ),
    (r"\bInstagram (\d+)", WEBVIEW, "Instagram"),
    (r"\bFBA[NV]/", WEBVIEW, "Facebook"),
    (r"\bLine/(\d+)", WEBVIEW, "LINE"),
    (r"\bNAVER\(inapp;", WEBVIEW, "NAVER"),
    (r"\bMicroMessenger/(\d+)", WEBVIEW, "WeChat"),
    (r"\bEdg(?:e|A|iOS)?/(\d+)", BROWSER, "Edge"),
    (r"\bOPR/(\d+)|\bOPiOS/(\d+)", BROWSER, "Opera"),
    (r"\bSamsungBrowser/(\d+)", BROWSER, "Samsung Internet"),
    (r"\bWhale/(\d+)", BROWSER, "Whale"),
    (r"\bYaBrowser/(\d+)", BROWSER, "Yandex Browser"),
    (r"\bFirefox/(\d+)|\bFxiOS/(\d+)", BROWSER, "Firefox"),
    (
        r"; wv\).*?\bChrome/(\d+)|; wv\)",
        WEBVIEW,
        "Android WebView",
    ),
    (r"\bCriOS/(\d+)|\bChrome/(\d+)", BROWSER, "Chrome"),
    (
        r"\bVersion/(\d+)[0-9.]* (?:Mobile/\S+ )?Safari/",
        BROWSER,
        "Safari",
    ),
    (
        r"\(i(?:Phone|Pad|Pod);.*\bAppleWebKit/.*\bMobile/",
        WEBVIEW,
        "iOS WebView",
    ),
    (
        r"\bMSIE (\d+)|\bTrident/.*\brv:(\d+)",
        BROWSER,
        "Internet Explorer",
    ),
];

// Chromium browsers that keep Chrome's User-Agent name themselves in Client
// Hints: `"Brave";v="141"`.
const BRANDS: &[(&str, &str)] = &[
    ("Brave", "Brave"),
    ("Microsoft Edge", "Edge"),
    ("Opera", "Opera"),
    ("Whale", "Whale"),
    ("Samsung Internet", "Samsung Internet"),
    ("Vivaldi", "Vivaldi"),
];

/// Every name a client may be given.
pub fn names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = RULES
        .iter()
        .map(|(_, _, name)| *name)
        .chain(BRANDS.iter().map(|(_, name)| *name))
        .chain(["Other", "No User-Agent"])
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

struct Rules {
    set: RegexSet,
    each: Vec<Regex>,
}

static COMPILED: LazyLock<Rules> = LazyLock::new(|| Rules {
    set: RegexSet::new(RULES.iter().map(|(pattern, _, _)| *pattern)).expect("valid client rules"),
    each: RULES
        .iter()
        .map(|(pattern, _, _)| Regex::new(pattern).expect("valid client rule"))
        .collect(),
});
static BRAND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""([^"]{1,40})";\s*v="(\d+)"#).expect("valid Client Hints brand pattern")
});
// A session sends few distinct User-Agents and many rows; each is read once.
static SEEN: LazyLock<Mutex<HashMap<u64, Client>>> = LazyLock::new(Default::default);

/// The client a request's User-Agent, `Sec-CH-UA` and `Sec-CH-UA-Platform`
/// name.
pub fn classify(user_agent: Option<&str>, brands: Option<&str>, platform: Option<&str>) -> Client {
    let mut hasher = DefaultHasher::new();
    (user_agent, brands, platform).hash(&mut hasher);
    let key = hasher.finish();
    let mut seen = SEEN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(client) = seen.get(&key) {
        return *client;
    }
    let client = read(user_agent.unwrap_or("").trim(), brands, platform);
    // ponytail: dropping every entry at the cap is enough for a few hundred
    // distinct clients; an LRU would keep the busiest.
    if seen.len() >= 4096 {
        seen.clear();
    }
    seen.insert(key, client);
    client
}

fn read(user_agent: &str, brands: Option<&str>, platform: Option<&str>) -> Client {
    let os = platform_os(platform).or_else(|| user_agent_os(user_agent));
    if user_agent.is_empty() {
        return Client {
            kind: UNKNOWN,
            name: "No User-Agent",
            version: None,
            os,
        };
    }
    let Some(index) = COMPILED.set.matches(user_agent).iter().next() else {
        return Client {
            kind: UNKNOWN,
            name: "Other",
            version: None,
            os,
        };
    };
    let (_, kind, name) = RULES[index];
    let version = COMPILED.each[index]
        .captures(user_agent)
        .and_then(|captures| {
            captures
                .iter()
                .skip(1)
                .flatten()
                .find_map(|group| group.as_str().parse().ok())
        });
    let mut client = Client {
        kind,
        name,
        version,
        os,
    };
    if name == "Chrome" {
        for captures in BRAND.captures_iter(brands.unwrap_or("")) {
            if let Some((_, name)) = BRANDS.iter().find(|(brand, _)| *brand == &captures[1]) {
                client.name = name;
                client.version = captures[2].parse().ok();
                break;
            }
        }
    }
    client
}

fn platform_os(platform: Option<&str>) -> Option<&'static str> {
    match platform?.trim().trim_matches('"') {
        "Windows" => Some("Windows"),
        "macOS" => Some("macOS"),
        "iOS" => Some("iOS"),
        "Android" => Some("Android"),
        "Chrome OS" | "Chromium OS" => Some("ChromeOS"),
        "Linux" => Some("Linux"),
        _ => None,
    }
}

fn user_agent_os(user_agent: &str) -> Option<&'static str> {
    if ["iPhone", "iPad", "iPod"]
        .iter()
        .any(|device| user_agent.contains(device))
    {
        Some("iOS")
    } else if user_agent.contains("Android") || user_agent.contains("Dalvik/") {
        Some("Android")
    } else if user_agent.contains("CrOS") {
        Some("ChromeOS")
    } else if user_agent.contains("Windows") {
        Some("Windows")
    } else if user_agent.contains("Mac OS X") || user_agent.contains("Macintosh") {
        Some("macOS")
    } else if user_agent.contains("Linux") {
        Some("Linux")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(
        user_agent: &str,
    ) -> (
        &'static str,
        &'static str,
        Option<u32>,
        Option<&'static str>,
    ) {
        let client = read(user_agent, None, None);
        (client.kind, client.name, client.version, client.os)
    }

    #[test]
    fn every_rule_compiles() {
        assert_eq!(COMPILED.each.len(), RULES.len());
        for (_, kind, _) in RULES {
            assert!(KINDS.contains(kind));
        }
    }

    // One User-Agent per rule, as each client sends it.
    #[test]
    fn user_agents_name_their_client() {
        let samples: &[(&str, (&str, &str, Option<u32>, Option<&str>))] = &[
            ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36", (BROWSER, "Chrome", Some(141), Some("macOS"))),
            ("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.3537.71", (BROWSER, "Edge", Some(141), Some("Windows"))),
            ("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 OPR/124.0.0.0", (BROWSER, "Opera", Some(124), Some("Windows"))),
            ("Mozilla/5.0 (Linux; Android 14; SM-S921N) AppleWebKit/537.36 (KHTML, like Gecko) SamsungBrowser/28.0 Chrome/130.0.0.0 Mobile Safari/537.36", (BROWSER, "Samsung Internet", Some(28), Some("Android"))),
            ("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Whale/4.33.325.17 Safari/537.36", (BROWSER, "Whale", Some(4), Some("Windows"))),
            ("Mozilla/5.0 (Macintosh; Intel Mac OS X 14.6; rv:143.0) Gecko/20100101 Firefox/143.0", (BROWSER, "Firefox", Some(143), Some("macOS"))),
            ("Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) FxiOS/143.0 Mobile/15E148 Safari/605.1.15", (BROWSER, "Firefox", Some(143), Some("iOS"))),
            ("Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/141.0.7390.96 Mobile/15E148 Safari/604.1", (BROWSER, "Chrome", Some(141), Some("iOS"))),
            ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15", (BROWSER, "Safari", Some(26), Some("macOS"))),
            ("Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.6 Mobile/15E148 Safari/604.1", (BROWSER, "Safari", Some(18), Some("iOS"))),
            ("Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148 ExampleApp/3.2", (WEBVIEW, "iOS WebView", None, Some("iOS"))),
            ("Mozilla/5.0 (Linux; Android 14; Pixel 8 Build/AP2A; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/141.0.0.0 Mobile Safari/537.36", (WEBVIEW, "Android WebView", Some(141), Some("Android"))),
            ("Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148 Instagram 400.0.0.39.68", (WEBVIEW, "Instagram", Some(400), Some("iOS"))),
            ("Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148 [FBAN/FBIOS;FBAV/520.0.0.39.97]", (WEBVIEW, "Facebook", None, Some("iOS"))),
            ("Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148 Safari Line/15.16.0", (WEBVIEW, "LINE", Some(15), Some("iOS"))),
            ("Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148 NAVER(inapp; search; 2000; 12.10.3)", (WEBVIEW, "NAVER", None, Some("iOS"))),
            ("Mozilla/5.0 (Linux; Android 14; wv) AppleWebKit/537.36 Chrome/141.0 Mobile Safari/537.36 MicroMessenger/8.0.50", (WEBVIEW, "WeChat", Some(8), Some("Android"))),
            ("okhttp/4.12.0", (APP, "OkHttp", Some(4), None)),
            ("Dalvik/2.1.0 (Linux; U; Android 14; Pixel 8 Build/AP2A)", (APP, "Android app", Some(2), Some("Android"))),
            ("ExampleApp/3.2 (iPhone; iOS 18.6; Scale/3.00) Alamofire/5.9.1", (APP, "Alamofire", Some(5), Some("iOS"))),
            ("ExampleApp/1203 CFNetwork/3826.600.41 Darwin/24.6.0", (APP, "Apple app", None, None)),
            ("curl/8.7.1", (TOOL, "curl", Some(8), None)),
            ("Wget/1.24.5", (TOOL, "Wget", Some(1), None)),
            ("HTTPie/3.2.4", (TOOL, "HTTPie", Some(3), None)),
            ("PostmanRuntime/7.43.0", (TOOL, "Postman", Some(7), None)),
            ("insomnia/10.1.1", (TOOL, "Insomnia", Some(10), None)),
            ("sqlmap/1.8.11#stable (https://example.com/)", (TOOL, "sqlmap", Some(1), None)),
            ("Mozilla/5.0 (compatible; Nmap Scripting Engine; https://example.com/)", (TOOL, "Nmap", None, None)),
            ("Mozilla/5.00 (Nikto/2.5.0) (Evasions:None) (Test:000001)", (TOOL, "Nikto", None, None)),
            ("WPScan v3.8.25 (https://example.com/)", (TOOL, "WPScan", Some(3), None)),
            ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/141.0.0.0 Safari/537.36", (TOOL, "Headless Chrome", Some(141), Some("macOS"))),
            ("python-requests/2.32.3", (LIBRARY, "python-requests", Some(2), None)),
            ("python-httpx/0.27.2", (LIBRARY, "HTTPX", Some(0), None)),
            ("Python/3.12 aiohttp/3.10.5", (LIBRARY, "aiohttp", Some(3), None)),
            ("Python-urllib/3.12", (LIBRARY, "Python urllib", Some(3), None)),
            ("Go-http-client/2.0", (LIBRARY, "Go net/http", Some(2), None)),
            ("Apache-HttpClient/4.5.14 (Java/17.0.12)", (LIBRARY, "Apache HttpClient", Some(4), None)),
            ("Java/17.0.12", (LIBRARY, "Java", Some(17), None)),
            ("axios/1.7.7", (LIBRARY, "axios", Some(1), None)),
            ("node", (LIBRARY, "Node.js", None, None)),
            ("Mozilla/5.0 (compatible; Googlebot/2.1; +https://example.com/)", (BOT, "Googlebot", Some(2), None)),
            ("Mozilla/5.0 (compatible; bingbot/2.0; +https://example.com/)", (BOT, "Bingbot", Some(2), None)),
            ("Mozilla/5.0 (compatible; Yeti/1.1; +https://example.com/)", (BOT, "Yeti", Some(1), None)),
            ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_5) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/13.1.1 Safari/605.1.15 (Applebot/0.1)", (BOT, "Applebot", Some(0), Some("macOS"))),
            ("DuckDuckBot/1.1; (+https://example.com/)", (BOT, "DuckDuckBot", Some(1), None)),
            ("Mozilla/5.0 (compatible; Baiduspider/2.0; +https://example.com/)", (BOT, "Baiduspider", Some(2), None)),
            ("Mozilla/5.0 (compatible; YandexBot/3.0; +https://example.com/)", (BOT, "YandexBot", Some(3), None)),
            ("facebookexternalhit/1.1 (+https://example.com/)", (BOT, "Facebook link preview", Some(1), None)),
            ("Twitterbot/1.0", (BOT, "Twitterbot", Some(1), None)),
            ("Slackbot-LinkExpanding 1.0 (+https://example.com/)", (BOT, "Slackbot", None, None)),
            ("Mozilla/5.0 (compatible; Discordbot/2.0; +https://example.com/)", (BOT, "Discordbot", Some(2), None)),
            ("LinkedInBot/1.0 (compatible; Mozilla/5.0; Apache-HttpClient +https://example.com/)", (BOT, "LinkedInBot", Some(1), None)),
            ("Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; GPTBot/1.2; +https://example.com/)", (BOT, "GPTBot", Some(1), None)),
            ("Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; ClaudeBot/1.0; +bot@example.com)", (BOT, "ClaudeBot", Some(1), None)),
            ("Mozilla/5.0 (compatible; AhrefsBot/7.0; +https://example.com/)", (BOT, "Other crawler", None, None)),
            ("Mozilla/5.0 (Windows NT 10.0; Trident/7.0; rv:11.0) like Gecko", (BROWSER, "Internet Explorer", Some(11), Some("Windows"))),
            ("ExampleTool", (UNKNOWN, "Other", None, None)),
            ("", (UNKNOWN, "No User-Agent", None, None)),
        ];
        for (user_agent, expected) in samples {
            assert_eq!(named(user_agent), *expected, "{user_agent}");
        }
    }

    #[test]
    fn client_hints_name_a_chromium_browser_and_its_platform() {
        let chrome = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
        let brave = classify(
            Some(chrome),
            Some(r#""Brave";v="141", "Not=A?Brand";v="8", "Chromium";v="141""#),
            Some(r#""Windows""#),
        );
        assert_eq!(
            (brave.name, brave.version, brave.os),
            ("Brave", Some(141), Some("Windows"))
        );
        let plain = classify(
            Some(chrome),
            Some(r#""Google Chrome";v="141", "Chromium";v="141""#),
            Some(r#""Linux""#),
        );
        assert_eq!(
            (plain.name, plain.os),
            ("Chrome", Some("Linux")),
            "the hint's platform wins"
        );
        // A hint never renames a browser that is not Chrome.
        let firefox = classify(
            Some("Mozilla/5.0 (X11; Linux x86_64; rv:143.0) Gecko/20100101 Firefox/143.0"),
            Some(r#""Brave";v="1""#),
            None,
        );
        assert_eq!(firefox.name, "Firefox");
    }
}
