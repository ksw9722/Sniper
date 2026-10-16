//! Front-end technologies retained responses declare: frameworks, CMSs, UI kits
//! and libraries, with a version when one is named.
//!
//! Response headers name few products and rarely a version: `X-Powered-By:
//! Next.js` carries none. Responses say more in three places, all read here
//! without running anything:
//! - a script's license banner (`/*! jQuery v3.7.1`) or runtime value
//!   (`window.next={version:"14.2.3"}`, React DOM's renderer registration);
//! - a page's markup: attributes frameworks render (`ng-version="17.1.0"`,
//!   `data-v-1a2b3c4d`), their root ids (`__nuxt`, `___gatsby`) and its
//!   `<meta name="generator">`;
//! - asset URLs: a CDN's `package@version` path, a versioned file name, or the
//!   directory a framework builds into (`/_next/static/`, `/wp-content/`).
//!
//! Wappalyzer's catalogue was the checklist of products and of the signals each
//! leaves; every rule here is written for this table, not taken from its data.
//! Only these anchors count: a bundle holds other libraries' `version:"…"`
//! literals too.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    sync::LazyLock,
};

use regex::{Regex, RegexSet};
use serde::Deserialize;
use uuid::Uuid;

use crate::{model::BodyEncoding, service_map::ServiceMapRow, session::BodyLocator};

/// Retained rows read from disk per map; the rest are read on later maps.
pub const MAX_READS: usize = 256;
/// Bytes copied or read per map, resident and retained together.
pub const MAX_BYTES_PER_MAP: usize = 32 * 1024 * 1024;
/// A serialized row larger than this is not read at all.
pub const MAX_ROW_BYTES: usize = 2 * 1024 * 1024;
/// Script and stylesheet URLs read from one page.
const MAX_PAGE_URLS: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detection {
    pub product_id: String,
    pub name: String,
    pub category: &'static str,
    /// None when the signal names the product but not its version.
    pub version: Option<String>,
    /// Empty for a front-end signal; `service_map_servers::SIGNALS` for one the
    /// responding origin's own headers, cookies or body gave.
    pub signal: &'static str,
}

/// What a row's body can declare: a script's technologies, a page's, or what
/// an error, a root or an API document response says about its servers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Script,
    Page,
    Data,
}

pub(crate) fn kind(row: &ServiceMapRow) -> Option<Kind> {
    let mime = row.content_type.as_deref()?;
    if mime.contains("javascript") || mime.contains("ecmascript") {
        Some(Kind::Script)
    } else if mime.contains("html") {
        Some(Kind::Page)
    } else if (mime.contains("json") || mime.contains("xml") || mime.starts_with("text/"))
        && (row.status.is_some_and(|status| status >= 400)
            || row.path == "/"
            || row.path.starts_with("/actuator")
            || ["/api-docs", "openapi.json", "swagger.json"]
                .iter()
                .any(|end| row.path.ends_with(end)))
    {
        // Only these are read: an API's every success would spend the budget.
        Some(Kind::Data)
    } else {
        None
    }
}

pub const META_FRAMEWORK: &str = "meta_framework";
pub const CMS: &str = "cms";
pub const FRAMEWORK: &str = "framework";
pub const UI: &str = "ui";
pub const LIBRARY: &str = "library";
pub const ANALYTICS: &str = "analytics";
/// Categories in the order a card presents them: what the site is built with
/// first, what it merely loads last.
pub const CATEGORIES: [&str; 6] = [META_FRAMEWORK, CMS, FRAMEWORK, UI, LIBRARY, ANALYTICS];

const PRODUCTS: &[(&str, &str, &str)] = &[
    ("next_js", "Next.js", META_FRAMEWORK),
    ("nuxt", "Nuxt", META_FRAMEWORK),
    ("sveltekit", "SvelteKit", META_FRAMEWORK),
    ("remix", "Remix", META_FRAMEWORK),
    ("gatsby", "Gatsby", META_FRAMEWORK),
    ("astro", "Astro", META_FRAMEWORK),
    ("wordpress", "WordPress", CMS),
    ("drupal", "Drupal", CMS),
    ("joomla", "Joomla", CMS),
    ("magento", "Magento", CMS),
    ("shopify", "Shopify", CMS),
    ("wix", "Wix", CMS),
    ("squarespace", "Squarespace", CMS),
    ("react", "React", FRAMEWORK),
    ("vue", "Vue.js", FRAMEWORK),
    ("angular", "Angular", FRAMEWORK),
    ("angularjs", "AngularJS", FRAMEWORK),
    ("svelte", "Svelte", FRAMEWORK),
    ("ember", "Ember.js", FRAMEWORK),
    ("preact", "Preact", FRAMEWORK),
    ("lit", "Lit", FRAMEWORK),
    ("qwik", "Qwik", FRAMEWORK),
    ("alpine_js", "Alpine.js", FRAMEWORK),
    ("htmx", "htmx", FRAMEWORK),
    ("hotwire_turbo", "Turbo", FRAMEWORK),
    ("livewire", "Livewire", FRAMEWORK),
    ("backbone", "Backbone.js", FRAMEWORK),
    ("knockout", "Knockout", FRAMEWORK),
    ("bootstrap", "Bootstrap", UI),
    ("vuetify", "Vuetify", UI),
    ("element_plus", "Element Plus", UI),
    ("element_ui", "Element UI", UI),
    ("quasar", "Quasar", UI),
    ("font_awesome", "Font Awesome", UI),
    ("jquery", "jQuery", LIBRARY),
    ("jquery_ui", "jQuery UI", LIBRARY),
    ("jquery_migrate", "jQuery Migrate", LIBRARY),
    ("lodash", "Lodash", LIBRARY),
    ("underscore", "Underscore.js", LIBRARY),
    ("moment", "Moment.js", LIBRARY),
    ("dayjs", "Day.js", LIBRARY),
    ("core_js", "core-js", LIBRARY),
    ("axios", "Axios", LIBRARY),
    ("socket_io", "Socket.IO", LIBRARY),
    ("dompurify", "DOMPurify", LIBRARY),
    ("chart_js", "Chart.js", LIBRARY),
    ("d3", "D3", LIBRARY),
    ("three_js", "three.js", LIBRARY),
    ("leaflet", "Leaflet", LIBRARY),
    ("highcharts", "Highcharts", LIBRARY),
    ("echarts", "ECharts", LIBRARY),
    ("gsap", "GSAP", LIBRARY),
    ("swiper", "Swiper", LIBRARY),
    ("video_js", "Video.js", LIBRARY),
    ("hls_js", "hls.js", LIBRARY),
    ("select2", "Select2", LIBRARY),
    ("popper", "Popper", LIBRARY),
    ("handlebars", "Handlebars", LIBRARY),
    ("modernizr", "Modernizr", LIBRARY),
    ("zepto", "Zepto", LIBRARY),
    ("prototype", "Prototype", LIBRARY),
    ("mootools", "MooTools", LIBRARY),
    ("pdf_js", "PDF.js", LIBRARY),
    ("tinymce", "TinyMCE", LIBRARY),
    ("ckeditor", "CKEditor", LIBRARY),
    ("quill", "Quill", LIBRARY),
    ("sweetalert2", "SweetAlert2", LIBRARY),
    ("hammer_js", "Hammer.js", LIBRARY),
    ("sentry", "Sentry", LIBRARY),
    ("google_tag_manager", "Google Tag Manager", ANALYTICS),
    ("google_analytics", "Google Analytics", ANALYTICS),
];

// Each rule names a product in PRODUCTS. `§` is the version: a digit, then
// letters, digits, `.`, `-` or `+`. A rule without one names the product only.
const SCRIPT_SIGNATURES: &[(&str, &str)] = &[
    ("react", r#"version:"§",rendererPackageName:"react-dom""#),
    ("react", r"@license React v§"),
    ("react", r"__REACT_DEVTOOLS_GLOBAL_HOOK__"),
    (
        "vue",
        r"@vue/(?:runtime-dom|runtime-core|shared|reactivity|compiler-core|compiler-dom) v§",
    ),
    ("vue", r"Vue\.js v§"),
    ("vue", r"(?m)^\s*\*\s*vue v§"),
    ("vue", r"__v_isRef"),
    ("nuxt", r#"get nuxt\(\)\{return"§""#),
    ("nuxt", r"__NUXT__"),
    ("angular", r"@license Angular v§"),
    ("angular", r"ɵcmp"),
    ("angularjs", r"@license AngularJS v§"),
    (
        "angularjs",
        r#"\{full:"§",major:[0-9]+,minor:[0-9]+,dot:[0-9]+"#,
    ),
    ("svelte", r#"__svelte\b[^;]{0,80}?\.add\(["']§["']\)"#),
    ("svelte", r"window\.__svelte\b"),
    (
        "ember",
        r"Ember - JavaScript Application Framework[\s\S]{0,400}?@version\s+§",
    ),
    ("ember", r"Ember - JavaScript Application Framework"),
    (
        "lit",
        r#"lit(?:Html|Element)Versions[^;]{0,60}?\.push\(["']§["']\)"#,
    ),
    ("alpine_js", r#"version:"§",flushAndStopDeferringMutations"#),
    ("htmx", r#"indicatorClass:\s*["']htmx-indicator["']"#),
    ("backbone", r"Backbone\.js §"),
    ("knockout", r"Knockout JavaScript library v§"),
    ("bootstrap", r"Bootstrap v§ \(https?://getbootstrap\.com"),
    ("vuetify", r"Vuetify v§"),
    ("element_plus", r"Element Plus v§"),
    ("quasar", r"Quasar Framework v§"),
    ("font_awesome", r"Font Awesome (?:Free|Pro) §"),
    // The minified banner and the full build's; a plugin saying it needs
    // "jQuery v1.7" is neither.
    ("jquery", r"/\*!\s*jQuery v§"),
    ("jquery", r"jQuery JavaScript Library v§"),
    ("jquery_ui", r"jQuery UI - v§"),
    ("jquery_migrate", r"jQuery Migrate v§"),
    (
        "lodash",
        r"Lodash <https://lodash\.com/>|Lodash lodash\.com/license",
    ),
    // Lodash's own banner names "Underscore.js 1.8.3"; Underscore's starts a
    // comment line.
    ("underscore", r"(?m)^\s*(?://|/?\*+!?)\s*Underscore\.js §"),
    ("moment", r"//! moment\.js\s*//! version : §"),
    ("core_js", r#"version:"§",mode:"(?:global|pure)""#),
    ("axios", r"Axios v§ Copyright"),
    ("socket_io", r"Socket\.IO v§"),
    ("dompurify", r"DOMPurify § \|"),
    ("chart_js", r"Chart\.js v§"),
    ("d3", r"https://d3js\.org v§"),
    ("three_js", r#"__THREE__\s*=\s*["']§["']"#),
    (
        "three_js",
        r"Multiple instances of Three\.js being imported",
    ),
    ("leaflet", r"Leaflet §, a JS library for interactive maps"),
    ("highcharts", r"Highcharts JS v§"),
    ("gsap", r"\* GSAP §"),
    ("swiper", r"\* Swiper §"),
    ("video_js", r"Video\.js § <"),
    ("select2", r"Select2 § \|"),
    ("popper", r"@popperjs/core v§"),
    ("handlebars", r"handlebars v§"),
    ("modernizr", r"modernizr § \(Custom Build\)"),
    ("zepto", r"Zepto v§"),
    ("prototype", r"Prototype JavaScript framework, version §"),
    ("mootools", r#"MooTools\s*=\s*\{\s*version\s*:\s*["']§"#),
    ("pdf_js", r#"pdfjsVersion\s*=\s*["']§"#),
    ("tinymce", r"TinyMCE version §"),
    ("ckeditor", r#"timestamp:"[A-Z0-9]{4}",version:"§"#),
    ("ckeditor", r"CKEDITOR_VERSION"),
    ("quill", r"Quill Editor v§"),
    ("sweetalert2", r"sweetalert2 v§"),
    ("hammer_js", r"Hammer\.JS - v§"),
    ("sentry", r"@sentry/(?:browser|react|vue|angular|nextjs) §"),
];

const PAGE_SIGNATURES: &[(&str, &str)] = &[
    ("angular", r#"\bng-version="§""#),
    (
        "angular",
        r"\b_ng(?:host|content)-[a-z0-9-]+|\bng-server-context=",
    ),
    (
        "angularjs",
        r"<[a-zA-Z][^>]*\s(?:data-)?ng-(?:app|controller)\b",
    ),
    (
        "next_js",
        r#"<script[^>]+id=["']__NEXT_DATA__["']|self\.__next_f\.push|/_next/static/"#,
    ),
    (
        "nuxt",
        r#"<div[^>]+id=["']__nuxt["']|window\.__NUXT__|/_nuxt/"#,
    ),
    ("react", r"\sdata-react(?:root|id)\b"),
    (
        "vue",
        r#"<[a-zA-Z][^>]*\sdata-v-[0-9a-f]{8}\b|\sdata-server-rendered=["']true["']"#,
    ),
    ("svelte", r#"\sclass=["'][^"']*\bsvelte-[a-z0-9]{5,8}\b"#),
    (
        "sveltekit",
        r"\sdata-sveltekit-[a-z-]+|__sveltekit_[a-z0-9]+|/_app/immutable/",
    ),
    ("gatsby", r#"<div[^>]+id=["']___gatsby["']"#),
    ("astro", r"<astro-island\b|/_astro/"),
    ("remix", r"__remixContext|__remixManifest"),
    ("qwik", r#"\sq:version=["']§["']"#),
    ("qwik", r"\sq:container="),
    (
        "ember",
        r#"\sclass=["'][^"']*\bember-view\b|<meta[^>]+name=["'][^"']*/config/environment["']"#,
    ),
    ("alpine_js", r"<[a-zA-Z][^>]*\sx-data(?:=|\s|>)"),
    (
        "htmx",
        r"<[a-zA-Z][^>]*\shx-(?:get|post|put|patch|delete|boost|target|swap|trigger)=",
    ),
    ("livewire", r"\swire:(?:model|click|submit|loading|id)\b"),
    ("hotwire_turbo", r"<turbo-frame\b|\sdata-turbo(?:-[a-z]+)?="),
    ("knockout", r#"<[a-zA-Z][^>]*\sdata-bind=["']"#),
    (
        "wordpress",
        r#"/wp-content/|/wp-includes/|<link[^>]+rel=["']https://api\.w\.org/["']"#,
    ),
    (
        "drupal",
        r"\sdata-drupal-[a-z-]+=|drupalSettings|Drupal\.settings|/sites/(?:default|all)/(?:files|modules|themes)/",
    ),
    ("joomla", r"/media/(?:jui|system/js)/"),
    (
        "magento",
        r"\sdata-mage-init=|Mage\.Cookies|/static/version[0-9]+/frontend/",
    ),
    ("shopify", r"cdn\.shopify\.com|\bShopify\.theme\b"),
    ("wix", r"static\.(?:wixstatic|parastorage)\.com"),
    (
        "squarespace",
        r"static1\.squarespace\.com|Static\.SQUARESPACE_CONTEXT",
    ),
    ("google_tag_manager", r"googletagmanager\.com/gtm\.js"),
    (
        "google_analytics",
        r"googletagmanager\.com/gtag/js|google-analytics\.com/(?:analytics|ga)\.js",
    ),
];

/// Directories and hosts a product serves its assets from.
const URL_MARKERS: &[(&str, &str)] = &[
    ("next_js", r"/_next/static/"),
    ("nuxt", r"/_nuxt/"),
    ("sveltekit", r"/_app/immutable/"),
    ("astro", r"/_astro/"),
    ("gatsby", r"/webpack-runtime-[0-9a-f]+\.js"),
    ("remix", r"/build/(?:_shared/|entry\.client-)"),
    ("wordpress", r"/wp-(?:content|includes)/"),
    (
        "drupal",
        r"/sites/(?:default|all)/(?:files|modules|themes)/|/core/misc/drupal(?:\.init)?\.js",
    ),
    ("joomla", r"/media/(?:jui|system/js)/"),
    ("magento", r"/static/version[0-9]+/frontend/"),
    ("shopify", r"cdn\.shopify\.com/|sdks\.shopifycdn\.com/"),
    ("wix", r"static\.(?:wixstatic|parastorage)\.com/"),
    ("squarespace", r"(?:static1|assets)\.squarespace\.com/"),
    ("livewire", r"/livewire(?:\.min)?\.js"),
    ("google_tag_manager", r"googletagmanager\.com/gtm\.js"),
    (
        "google_analytics",
        r"googletagmanager\.com/gtag/js|google-analytics\.com/(?:analytics|ga)\.js",
    ),
];

const VERSION: &str = r"([0-9][0-9A-Za-z.+-]{0,39})";

struct Signatures {
    set: RegexSet,
    /// The rules that name a version, compiled again to capture it.
    each: Vec<Option<Regex>>,
    ids: Vec<&'static str>,
}

fn compile(rules: &'static [(&'static str, &'static str)]) -> Signatures {
    let patterns: Vec<String> = rules
        .iter()
        .map(|(_, pattern)| pattern.replace('§', VERSION))
        .collect();
    Signatures {
        set: RegexSet::new(&patterns).expect("valid front-end signatures"),
        each: rules
            .iter()
            .zip(&patterns)
            .map(|((_, rule), pattern)| {
                rule.contains('§')
                    .then(|| Regex::new(pattern).expect("valid front-end signature"))
            })
            .collect(),
        ids: rules.iter().map(|(id, _)| *id).collect(),
    }
}

static SCRIPTS: LazyLock<Signatures> = LazyLock::new(|| compile(SCRIPT_SIGNATURES));
static PAGES: LazyLock<Signatures> = LazyLock::new(|| compile(PAGE_SIGNATURES));
static MARKERS: LazyLock<Signatures> = LazyLock::new(|| compile(URL_MARKERS));
static GENERATOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<meta\b[^>]*\bname\s*=\s*["']generator["'][^>]*>"#)
        .expect("valid generator pattern")
});
static CONTENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\bcontent\s*=\s*["']([^"']{1,120})["']"#).expect("valid content pattern")
});
static ASSET_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<(?:script|link)\b[^>]*?\s(?:src|href)\s*=\s*["']([^"'\s<>]{1,512})["']"#)
        .expect("valid asset URL pattern")
});
// A package and its version as CDNs path them, a version in a file name, and
// WordPress's own version on the core assets it serves. The paths, not the
// hosts, are matched, so a company's mirror of a public CDN counts too:
// `/npm/vue@3.4.21/` and unpkg's `/vue@3.4.21/`, cdnjs's and Google's
// `/ajax/libs/jquery/3.7.1/`, jQuery's `/ui/1.13.2/jquery-ui.js`.
// Under `/npm/` a dist tag (`@latest`) names the package too; elsewhere only
// a version does, so `/users/jquery@example.com/` is no package.
static CDN_PACKAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)/(?:npm/((?:@[a-z0-9._-]+/)?[a-z0-9._-]+)@(?:([0-9][0-9a-z.+-]{0,39})|[a-z0-9._-]*)|((?:@[a-z0-9._-]+/)?[a-z0-9._-]+)@([0-9][0-9a-z.+-]{0,39}))(?:[/?#]|$)")
        .expect("valid CDN package pattern")
});
static CDN_LIBRARY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)/ajax/libs/([a-z0-9._-]+)/([0-9][0-9a-z.+-]{0,39})/")
        .expect("valid CDN library pattern")
});
static JQUERY_UI: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)/ui/([0-9][0-9.]*[0-9])/jquery-ui(?:\.min)?\.(?:js|css)")
        .expect("valid jQuery UI pattern")
});
static FILE_VERSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)/(jquery-ui|jquery-migrate|jquery|bootstrap|vue|react-dom|react|angular|lodash|moment|knockout|backbone|underscore|handlebars|modernizr|select2|swiper|d3|leaflet)[.-]v?([0-9]+\.[0-9]+(?:\.[0-9]+)?)(?:[.-](?:slim|min|bundle|umd|prod|production))*\.(?:js|css)(?:$|[?#])")
        .expect("valid versioned file pattern")
});
static WORDPRESS_ASSET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)/wp-includes/(?:(js/jquery/jquery)(?:\.min)?\.js|js/wp-emoji-release\.min\.js|css/dist/block-library/style(?:\.min)?\.css)\?ver=([0-9][0-9.]*[0-9])")
        .expect("valid WordPress asset pattern")
});

/// The product a CDN package or a versioned file is.
fn package_product(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_lowercase().as_str() {
        "react" | "react-dom" => "react",
        "vue" | "@vue/runtime-dom" | "@vue/runtime-core" => "vue",
        "nuxt" => "nuxt",
        "@angular/core" => "angular",
        "angular" | "angular.js" | "angularjs" => "angularjs",
        "svelte" => "svelte",
        "ember-source" | "ember.js" => "ember",
        "preact" => "preact",
        "lit" | "lit-html" | "lit-element" => "lit",
        "alpinejs" => "alpine_js",
        "htmx.org" | "htmx" => "htmx",
        "@hotwired/turbo" => "hotwire_turbo",
        "backbone" | "backbone.js" => "backbone",
        "knockout" => "knockout",
        "bootstrap" | "twitter-bootstrap" => "bootstrap",
        "vuetify" => "vuetify",
        "element-plus" => "element_plus",
        "element-ui" => "element_ui",
        "quasar" => "quasar",
        "font-awesome" | "@fortawesome/fontawesome-free" => "font_awesome",
        "jquery" => "jquery",
        "jqueryui" | "jquery-ui" | "jquery-ui-dist" => "jquery_ui",
        "jquery-migrate" => "jquery_migrate",
        "lodash" | "lodash.js" => "lodash",
        "underscore" | "underscore.js" => "underscore",
        "moment" | "moment.js" => "moment",
        "dayjs" => "dayjs",
        "core-js" | "core-js-bundle" => "core_js",
        "axios" => "axios",
        "socket.io" | "socket.io-client" => "socket_io",
        "dompurify" => "dompurify",
        "chart.js" => "chart_js",
        "d3" | "d3js" => "d3",
        "three" | "three.js" | "threejs" => "three_js",
        "leaflet" => "leaflet",
        "highcharts" => "highcharts",
        "echarts" => "echarts",
        "gsap" => "gsap",
        "swiper" => "swiper",
        "video.js" => "video_js",
        "hls.js" => "hls_js",
        "select2" => "select2",
        "@popperjs/core" | "popper.js" => "popper",
        "handlebars" | "handlebars.js" => "handlebars",
        "modernizr" => "modernizr",
        "zepto" | "zepto.js" => "zepto",
        "prototype" => "prototype",
        "mootools" => "mootools",
        "pdfjs-dist" | "pdf.js" => "pdf_js",
        "tinymce" => "tinymce",
        "ckeditor" | "ckeditor5" => "ckeditor",
        "quill" => "quill",
        "sweetalert2" => "sweetalert2",
        "hammer.js" | "hammerjs" => "hammer_js",
        "@sentry/browser" => "sentry",
        _ => return None,
    })
}

/// A product in the table, with the version found for it.
pub(crate) fn known(id: &str, version: Option<&str>) -> Option<Detection> {
    let (id, name, category) = PRODUCTS.iter().find(|(known, _, _)| *known == id)?;
    Some(Detection {
        product_id: (*id).into(),
        name: (*name).into(),
        category,
        version: version.filter(|v| valid_version(v)).map(str::to_string),
        signal: "",
    })
}

/// Keeps one entry per product: the first version found, or none until one is.
pub(crate) fn merge(found: &mut Vec<Detection>, detection: Detection) {
    match found
        .iter_mut()
        .find(|item| item.product_id == detection.product_id)
    {
        Some(item) if item.version.is_none() => item.version = detection.version,
        Some(_) => {}
        None => found.push(detection),
    }
}

fn scan(signatures: &Signatures, text: &str, found: &mut Vec<Detection>) {
    for index in signatures.set.matches(text).iter() {
        let version = signatures.each[index]
            .as_ref()
            .and_then(|rule| rule.captures(text))
            .and_then(|captures| captures.get(1))
            .map(|version| version.as_str());
        if let Some(detection) = known(signatures.ids[index], version) {
            merge(found, detection);
        }
    }
}

/// Every technology this body declares, one entry per product.
pub(crate) fn detect(body: &str, kind: Kind) -> Vec<Detection> {
    let mut found = Vec::new();
    match kind {
        Kind::Script => {
            if let Some(detection) = next_version(body).and_then(|v| known("next_js", Some(&v))) {
                merge(&mut found, detection);
            }
            scan(&SCRIPTS, body, &mut found);
        }
        Kind::Page => {
            scan(&PAGES, body, &mut found);
            for tag in GENERATOR.find_iter(body).take(4) {
                if let Some(detection) = CONTENT
                    .captures(tag.as_str())
                    .and_then(|captures| generator(&captures[1]))
                {
                    merge(&mut found, detection);
                }
            }
            for url in ASSET_URL.captures_iter(body).take(MAX_PAGE_URLS) {
                for detection in detect_url(&url[1]) {
                    merge(&mut found, detection);
                }
            }
            for detection in crate::service_map_servers::detect_body(body) {
                merge(&mut found, detection);
            }
        }
        Kind::Data => {
            for detection in crate::service_map_servers::detect_body(body) {
                merge(&mut found, detection);
            }
        }
    }
    found
}

/// What an asset URL names: a CDN package and version, a versioned file, or a
/// product's build directory or host. A row's own URL is read this way without
/// its body, as are the scripts and stylesheets a page links.
pub(crate) fn detect_url(url: &str) -> Vec<Detection> {
    let mut found = Vec::new();
    let mut add = |id: Option<&str>, version: Option<&str>| {
        if let Some(detection) = id.and_then(|id| known(id, version)) {
            merge(&mut found, detection);
        }
    };
    if let Some(captures) = CDN_PACKAGE.captures(url) {
        add(
            captures
                .get(1)
                .or_else(|| captures.get(3))
                .and_then(|package| package_product(package.as_str())),
            captures
                .get(2)
                .or_else(|| captures.get(4))
                .map(|v| v.as_str()),
        );
    }
    for captures in [&CDN_LIBRARY, &FILE_VERSION]
        .into_iter()
        .filter_map(|rule| rule.captures(url))
    {
        add(
            package_product(&captures[1]),
            captures.get(2).map(|v| v.as_str()),
        );
    }
    if let Some(captures) = JQUERY_UI.captures(url) {
        add(Some("jquery_ui"), Some(&captures[1]));
    }
    if let Some(captures) = WORDPRESS_ASSET.captures(url) {
        // WordPress versions its core assets with its own release, and the
        // jQuery it bundles with jQuery's.
        let product = if captures.get(1).is_some() {
            "jquery"
        } else {
            "wordpress"
        };
        add(Some(product), captures.get(2).map(|v| v.as_str()));
    }
    for index in MARKERS.set.matches(url).iter() {
        add(Some(MARKERS.ids[index]), None);
    }
    found
}

/// A generator's name and version: "WordPress 6.5.2", "Astro v4.0.3",
/// "Drupal 10 (https://www.drupal.org)", or a name alone. A product in the
/// table keeps its own name.
fn generator(content: &str) -> Option<Detection> {
    let mut words = Vec::new();
    let mut version = None;
    for word in content.split_whitespace().take(6) {
        let candidate = word
            .strip_prefix(['v', 'V'])
            .filter(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
            .unwrap_or(word);
        if candidate.starts_with(|c: char| c.is_ascii_digit()) {
            version = Some(
                candidate
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
                    .collect::<String>(),
            );
            break;
        }
        words.push(word);
    }
    let name = words.join(" ");
    let lower = name.to_ascii_lowercase();
    if let Some((id, _, _)) = PRODUCTS.iter().find(|(_, product, _)| {
        let product = product.to_ascii_lowercase();
        lower.starts_with(&product)
            && !lower[product.len()..].starts_with(|c: char| c.is_ascii_alphanumeric())
    }) {
        return known(id, version.as_deref());
    }
    let product_id: String = lower
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    let name_ok = name.len() <= 40
        && name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '!' | '+' | '_' | '-'));
    (name_ok && !product_id.is_empty() && product_id.len() <= 40).then(|| Detection {
        product_id,
        name,
        category: CMS,
        version: version.filter(|v| valid_version(v)),
        signal: "",
    })
}

pub(crate) fn valid_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 40
        && version.starts_with(|c: char| c.is_ascii_digit())
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+'))
}

/// The version Next.js's client runtime names in this script, if it does.
fn next_version(script: &str) -> Option<String> {
    const ANCHOR: &str = "window.next={version:";
    for (at, _) in script.match_indices(ANCHOR) {
        let rest = &script[at + ANCHOR.len()..];
        // The App Router writes the literal. The Pages Router names a variable
        // assigned earlier in the bundle. ponytail: the nearest `name="…"`
        // before the anchor is taken as its value; a parser would be exact.
        if let Some(literal) = rest.strip_prefix('"') {
            if let Some(version) = literal.split('"').next().filter(|v| valid_version(v)) {
                return Some(version.to_string());
            }
            continue;
        }
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$'))
            .collect();
        if name.is_empty() {
            continue;
        }
        let mut start = at.saturating_sub(4096);
        while !script.is_char_boundary(start) {
            start += 1;
        }
        let before = &script[start..at];
        if let Some(found) = before.rfind(&format!("{name}=\"")) {
            let literal = &before[found + name.len() + 2..];
            if let Some(version) = literal.split('"').next().filter(|v| valid_version(v)) {
                return Some(version.to_string());
            }
        }
    }
    None
}

// Only the response body is kept; serde skips every other field of the row.
#[derive(Deserialize)]
struct DiskRow {
    id: Uuid,
    response: Option<DiskResponse>,
}

#[derive(Deserialize)]
struct DiskResponse {
    #[serde(default)]
    body_preview: String,
    #[serde(default)]
    body_encoding: BodyEncoding,
}

/// One retained row's response body, or None when it cannot be shown to be
/// this record's UTF-8 body.
pub(crate) fn read_body(file: &mut File, locator: BodyLocator, record_id: Uuid) -> Option<String> {
    let len = locator.len as usize;
    if len == 0 || len > MAX_ROW_BYTES {
        return None;
    }
    file.seek(SeekFrom::Start(locator.offset)).ok()?;
    let mut bytes = vec![0u8; len];
    file.read_exact(&mut bytes).ok()?;
    let row: DiskRow = serde_json::from_slice(&bytes).ok()?;
    let response = row.response?;
    (row.id == record_id && response.body_encoding == BodyEncoding::Utf8)
        .then_some(response.body_preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(items: Vec<Detection>) -> Vec<(String, Option<String>)> {
        items
            .into_iter()
            .map(|item| (item.product_id, item.version))
            .collect()
    }

    fn one(id: &str, version: Option<&str>) -> (String, Option<String>) {
        (id.into(), version.map(str::to_string))
    }

    #[test]
    fn every_rule_compiles_and_names_a_product() {
        for (id, _) in SCRIPT_SIGNATURES
            .iter()
            .chain(PAGE_SIGNATURES)
            .chain(URL_MARKERS)
        {
            assert!(known(id, None).is_some(), "{id} is not in PRODUCTS");
        }
        for (_, _, category) in PRODUCTS {
            assert!(CATEGORIES.contains(category));
        }
        assert_eq!(SCRIPTS.ids.len(), SCRIPT_SIGNATURES.len());
        assert_eq!(PAGES.ids.len(), PAGE_SIGNATURES.len());
        assert_eq!(MARKERS.ids.len(), URL_MARKERS.len());
    }

    #[test]
    fn next_version_reads_the_runtime_literal_or_its_variable() {
        // The App Router form, as nextjs.org ships it.
        assert_eq!(
            next_version(r#"n()}window.next={version:"16.5.0-canary.9",appDir:!0},("function""#),
            Some("16.5.0-canary.9".into())
        );
        // The Pages Router form: a variable assigned earlier in the bundle.
        assert_eq!(
            next_version(r#"let s="13.4.7",o=null;function f(){}window.next={version:s,router:r}"#),
            Some("13.4.7".into())
        );
        assert_eq!(
            next_version(r#"window.next={version:"<script>",appDir:!0}"#),
            None
        );
        assert_eq!(next_version(r#"window.next={version:x,router:r}"#), None);
    }

    // One sample per script rule, as each library writes it.
    #[test]
    fn scripts_name_their_technologies_by_banner_or_runtime_value() {
        let samples: &[(&str, &str, Option<&str>)] = &[
            (r#"{bundleType:0,version:"18.2.0",rendererPackageName:"react-dom"}"#, "react", Some("18.2.0")),
            ("/** @license React v17.0.2\n * react-dom.production.min.js", "react", Some("17.0.2")),
            (r#""undefined"!=typeof __REACT_DEVTOOLS_GLOBAL_HOOK__"#, "react", None),
            ("/**\n* @vue/shared v3.4.21\n* (c) 2018-present Yuxi (Evan) You\n**/", "vue", Some("3.4.21")),
            ("/*!\n * Vue.js v2.7.16\n * (c) 2014-2023 Evan You", "vue", Some("2.7.16")),
            ("/**\n * vue v3.4.21\n * (c) 2018-present Yuxi (Evan) You", "vue", Some("3.4.21")),
            (r#"function Ze(e){return!!(e&&!0===e.__v_isRef)}"#, "vue", None),
            (r#"versions:{get nuxt(){return"3.8.2"},get vue(){return e.vueApp.version}}"#, "nuxt", Some("3.8.2")),
            ("window.__NUXT__", "nuxt", None),
            ("/**\n * @license Angular v17.1.0\n * (c) 2010-2022 Google LLC.", "angular", Some("17.1.0")),
            ("static{this.ɵcmp=ve({type:t,selectors:[[\"app-root\"]]})}", "angular", None),
            ("/*\n AngularJS v1.8.2\n @license AngularJS v1.8.2\n (c) 2010-2020 Google LLC.", "angularjs", Some("1.8.2")),
            (r#"{full:"1.8.3",major:1,minor:8,dot:3,codeName:"ultimate-farewell"}"#, "angularjs", Some("1.8.3")),
            (r#"typeof window<"u"&&(window.__svelte||(window.__svelte={v:new Set})).v.add("4")"#, "svelte", Some("4")),
            ("/*!\n * @overview  Ember - JavaScript Application Framework\n * @license   Licensed under MIT license\n * @version   4.12.0\n */", "ember", Some("4.12.0")),
            (r#"(globalThis.litHtmlVersions??=[]).push("3.1.0")"#, "lit", Some("3.1.0")),
            (r#"version:"3.13.3",flushAndStopDeferringMutations:Ft"#, "alpine_js", Some("3.13.3")),
            (r#"config:{historyEnabled:!0,indicatorClass:"htmx-indicator",requestClass:"htmx-request"}"#, "htmx", None),
            ("//     Backbone.js 1.4.1\n", "backbone", Some("1.4.1")),
            ("/*!\n * Knockout JavaScript library v3.5.1\n", "knockout", Some("3.5.1")),
            ("/*!\n  * Bootstrap v5.3.2 (https://getbootstrap.com/)", "bootstrap", Some("5.3.2")),
            ("/*!\n* Vuetify v3.4.0\n* Forged by John Leider", "vuetify", Some("3.4.0")),
            ("/*! Element Plus v2.4.3 */", "element_plus", Some("2.4.3")),
            ("/*!\n * Quasar Framework v2.14.2\n * (c) 2015-present Razvan Stoenescu", "quasar", Some("2.14.2")),
            ("/*!\n * Font Awesome Free 6.5.1 by @fontawesome", "font_awesome", Some("6.5.1")),
            ("/*! jQuery v3.7.1 | (c) OpenJS Foundation and other contributors */", "jquery", Some("3.7.1")),
            ("/*! jQuery v3.7.1 -ajax,-ajax/jsonp | (c) OpenJS Foundation */", "jquery", Some("3.7.1")),
            ("/*!\n * jQuery JavaScript Library v3.6.0\n", "jquery", Some("3.6.0")),
            ("/*! jQuery UI - v1.13.2 - 2022-07-14", "jquery_ui", Some("1.13.2")),
            ("/*! jQuery Migrate v3.4.1 | (c) OpenJS Foundation */", "jquery_migrate", Some("3.4.1")),
            ("/**\n * @license\n * Lodash <https://lodash.com/>\n", "lodash", None),
            ("//     Underscore.js 1.13.6\n//     https://underscorejs.org", "underscore", Some("1.13.6")),
            ("//! moment.js\n//! version : 2.29.4", "moment", Some("2.29.4")),
            (r#"push({version:"3.38.1",mode:"global",copyright:"© 2014-2024"})"#, "core_js", Some("3.38.1")),
            ("/*! Axios v1.6.7 Copyright (c) 2024 Matt Zabriskie and contributors */", "axios", Some("1.6.7")),
            ("/*!\n * Socket.IO v4.7.2\n * (c) 2014-2023 Guillermo Rauch", "socket_io", Some("4.7.2")),
            ("/*! @license DOMPurify 3.0.6 | (c) Cure53 and other contributors */", "dompurify", Some("3.0.6")),
            ("/*!\n * Chart.js v4.4.0\n * https://www.chartjs.org", "chart_js", Some("4.4.0")),
            ("// https://d3js.org v7.8.5 Copyright 2010-2023 Mike Bostock", "d3", Some("7.8.5")),
            (r#"window.__THREE__="160""#, "three_js", Some("160")),
            ("console.warn(\"WARNING: Multiple instances of Three.js being imported.\")", "three_js", None),
            ("/* @preserve\n * Leaflet 1.9.4, a JS library for interactive maps. https://leafletjs.com", "leaflet", Some("1.9.4")),
            ("/*\n Highcharts JS v11.2.0 (2023-10-30)\n", "highcharts", Some("11.2.0")),
            ("/*!\n * GSAP 3.12.2\n * https://greensock.com", "gsap", Some("3.12.2")),
            ("/**\n * Swiper 11.0.5\n * Most modern mobile touch slider", "swiper", Some("11.0.5")),
            ("/**\n * @license\n * Video.js 8.6.1 <http://videojs.com/>", "video_js", Some("8.6.1")),
            ("/*! Select2 4.0.13 | https://github.com/select2/select2/blob/master/LICENSE.md */", "select2", Some("4.0.13")),
            ("/**\n * @popperjs/core v2.11.8 - MIT License\n */", "popper", Some("2.11.8")),
            ("/**!\n\n @license\n handlebars v4.7.8\n", "handlebars", Some("4.7.8")),
            ("/*! modernizr 3.6.0 (Custom Build) | MIT *", "modernizr", Some("3.6.0")),
            ("/* Zepto v1.2.0 - zepto event ajax form ie - zeptojs.com/license */", "zepto", Some("1.2.0")),
            ("/*  Prototype JavaScript framework, version 1.7.3\n", "prototype", Some("1.7.3")),
            (r#"this.MooTools={version:"1.6.0",build:"529422872adfff401b901b8b6c7ca5114ee95e2b"}"#, "mootools", Some("1.6.0")),
            (r#"const pdfjsVersion="3.11.174""#, "pdf_js", Some("3.11.174")),
            ("/**\n * TinyMCE version 6.7.0 (2023-08-30)\n */", "tinymce", Some("6.7.0")),
            (r#"d={timestamp:"N3OC",version:"4.22.1 (Standard)",revision:"7a9e5f8"}"#, "ckeditor", Some("4.22.1")),
            ("window.CKEDITOR_VERSION=e", "ckeditor", None),
            ("/*!\n * Quill Editor v1.3.7\n * https://quilljs.com/", "quill", Some("1.3.7")),
            ("/*!\n* sweetalert2 v11.10.1\n* Released under the MIT License.\n*/", "sweetalert2", Some("11.10.1")),
            ("/*! Hammer.JS - v2.0.7 - 2016-04-22", "hammer_js", Some("2.0.7")),
            ("/*! @sentry/browser 7.81.1 (6f1c1f0) | https://github.com/getsentry/sentry-javascript */", "sentry", Some("7.81.1")),
            (r#"n()}window.next={version:"14.2.3",appDir:!0}"#, "next_js", Some("14.2.3")),
        ];
        for (sample, id, version) in samples {
            assert!(
                found(detect(sample, Kind::Script)).contains(&one(id, *version)),
                "{id} {version:?} in {sample}: {:?}",
                found(detect(sample, Kind::Script))
            );
        }
        // Neighbouring literals name nothing: another library's version field,
        // a plugin's requirement, Lodash's credit to Underscore.
        for sample in [
            r#"other.version:"9.9.9""#,
            "requires jQuery v1.7 or later",
            "/** @license Lodash lodash.com/license | Underscore.js 1.8.3 underscorejs.org/LICENSE */",
            "console.log(1)",
        ] {
            let names: Vec<_> = found(detect(sample, Kind::Script))
                .into_iter()
                .filter(|(id, _)| id != "lodash")
                .collect();
            assert!(names.is_empty(), "{sample}: {names:?}");
        }
        // Page markup is not read from scripts.
        assert!(detect(r#"<app ng-version="17.1.0">"#, Kind::Script).is_empty());
    }

    // One sample per page rule, the generator meta and a linked asset.
    #[test]
    fn pages_name_their_technologies_by_markup_generator_and_assets() {
        let samples: &[(&str, &str, Option<&str>)] = &[
            (
                r#"<app-root ng-version="17.1.0"></app-root>"#,
                "angular",
                Some("17.1.0"),
            ),
            (r#"<div _ngcontent-ng-c2087410263="">"#, "angular", None),
            (r#"<html ng-app="shop">"#, "angularjs", None),
            (r#"<div data-ng-controller="Cart">"#, "angularjs", None),
            (
                r#"<script id="__NEXT_DATA__" type="application/json">"#,
                "next_js",
                None,
            ),
            (
                r#"<script>self.__next_f.push([1,""])</script>"#,
                "next_js",
                None,
            ),
            (r#"<div id="__nuxt"><div id="__layout">"#, "nuxt", None),
            (r#"<div id="root" data-reactroot="">"#, "react", None),
            (r#"<div data-v-1a2b3c4d class="card">"#, "vue", None),
            (r#"<div id="app" data-server-rendered="true">"#, "vue", None),
            (r#"<h1 class="title svelte-1x2y3z4">"#, "svelte", None),
            (
                r#"<body data-sveltekit-preload-data="hover">"#,
                "sveltekit",
                None,
            ),
            (
                r#"<div id="___gatsby"><div id="gatsby-focus-wrapper">"#,
                "gatsby",
                None,
            ),
            (
                r#"<astro-island uid="Z1" component-url="/_astro/Counter.js">"#,
                "astro",
                None,
            ),
            (
                r#"<script>window.__remixContext = {}</script>"#,
                "remix",
                None,
            ),
            (
                r#"<html q:container="paused" q:version="1.4.0">"#,
                "qwik",
                Some("1.4.0"),
            ),
            (r#"<div id="ember123" class="ember-view">"#, "ember", None),
            (r#"<div x-data="{ open: false }">"#, "alpine_js", None),
            (
                r#"<button hx-post="/clicked" hx-swap="outerHTML">"#,
                "htmx",
                None,
            ),
            (r#"<input wire:model="search">"#, "livewire", None),
            (r#"<turbo-frame id="messages">"#, "hotwire_turbo", None),
            (r#"<span data-bind="text: name"></span>"#, "knockout", None),
            (
                r#"<link rel="https://api.w.org/" href="https://example.com/wp-json/">"#,
                "wordpress",
                None,
            ),
            (
                r#"<script type="application/json" data-drupal-selector="drupal-settings-json">"#,
                "drupal",
                None,
            ),
            (
                r#"<link href="/media/system/js/core.js" rel="preload">"#,
                "joomla",
                None,
            ),
            (
                r#"<div data-mage-init='{"collapsible":{}}'>"#,
                "magento",
                None,
            ),
            (
                r#"<script>Shopify.theme = {"name":"Dawn"};</script>"#,
                "shopify",
                None,
            ),
            (
                r#"<img src="https://static.wixstatic.com/media/a.png">"#,
                "wix",
                None,
            ),
            (
                r#"<script>Static.SQUARESPACE_CONTEXT = {};</script>"#,
                "squarespace",
                None,
            ),
            (
                r#"j.src='https://www.googletagmanager.com/gtm.js?id='+i"#,
                "google_tag_manager",
                None,
            ),
            (
                r#"<script async src="https://www.googletagmanager.com/gtag/js?id=G-1"></script>"#,
                "google_analytics",
                None,
            ),
            (
                r#"<meta name="generator" content="WordPress 6.5.2">"#,
                "wordpress",
                Some("6.5.2"),
            ),
            (
                r#"<meta content="Astro v4.0.3" name="generator">"#,
                "astro",
                Some("4.0.3"),
            ),
            (
                r#"<meta name="generator" content="Joomla! - Open Source Content Management">"#,
                "joomla",
                None,
            ),
            (
                r#"<meta name="generator" content="Hugo 0.120.4">"#,
                "hugo",
                Some("0.120.4"),
            ),
            (
                r#"<script src="https://cdn.example.com/npm/vue@3.4.21/dist/vue.global.prod.js"></script>"#,
                "vue",
                Some("3.4.21"),
            ),
        ];
        for (sample, id, version) in samples {
            assert!(
                found(detect(sample, Kind::Page)).contains(&one(id, *version)),
                "{id} {version:?} in {sample}: {:?}",
                found(detect(sample, Kind::Page))
            );
        }
        let drupal = generator("Drupal 10 (https://www.drupal.org)").unwrap();
        assert_eq!(
            (drupal.name, drupal.version),
            ("Drupal".into(), Some("10".into()))
        );
        assert_eq!(
            generator("Wix.com Website Builder").unwrap().product_id,
            "wix"
        );
        assert_eq!(generator("VuePress 2.0.0").unwrap().product_id, "vuepress");
        assert!(detect("<p>plain page</p>", Kind::Page).is_empty());
    }

    #[test]
    fn asset_urls_name_packages_versions_and_build_directories() {
        let samples: &[(&str, &str, Option<&str>)] = &[
            ("https://cdn.example.com/npm/vue@3.4.21/dist/vue.global.prod.js", "vue", Some("3.4.21")),
            ("https://unpkg.example.com/react-dom@18.2.0/umd/react-dom.production.min.js", "react", Some("18.2.0")),
            ("https://cdn.example.com/npm/@popperjs/core@2.11.8/dist/umd/popper.min.js", "popper", Some("2.11.8")),
            ("https://cdn.example.com/npm/bootstrap@5.3.2/dist/css/bootstrap.min.css", "bootstrap", Some("5.3.2")),
            ("https://cdn.example.com/npm/htmx.org@latest", "htmx", None),
            ("https://cdnjs.example.com/ajax/libs/lodash.js/4.17.21/lodash.min.js", "lodash", Some("4.17.21")),
            ("https://ajax.example.com/ajax/libs/angularjs/1.8.2/angular.min.js", "angularjs", Some("1.8.2")),
            ("https://ajax.example.com/ajax/libs/jqueryui/1.13.2/jquery-ui.min.js", "jquery_ui", Some("1.13.2")),
            ("https://code.example.com/jquery-3.7.1.slim.min.js", "jquery", Some("3.7.1")),
            ("https://code.example.com/ui/1.13.2/jquery-ui.js", "jquery_ui", Some("1.13.2")),
            ("https://example.com/js/jquery-3.6.0.min.js", "jquery", Some("3.6.0")),
            ("https://example.com/js/jquery-ui-1.12.1.min.js", "jquery_ui", Some("1.12.1")),
            ("https://example.com/static/angular-1.8.2.min.js?v=1", "angularjs", Some("1.8.2")),
            ("https://example.com/wp-includes/js/jquery/jquery.min.js?ver=3.7.1", "jquery", Some("3.7.1")),
            ("https://example.com/wp-includes/css/dist/block-library/style.min.css?ver=6.5.2", "wordpress", Some("6.5.2")),
            ("https://example.com/wp-content/themes/twentyone/style.css", "wordpress", None),
            ("https://example.com/_next/static/chunks/main-app-abc.js", "next_js", None),
            ("https://example.com/_nuxt/entry.4f2a.js", "nuxt", None),
            ("https://example.com/_app/immutable/entry/start.js", "sveltekit", None),
            ("https://example.com/_astro/hoisted.js", "astro", None),
            ("https://example.com/webpack-runtime-3f2a9c1b.js", "gatsby", None),
            ("https://example.com/build/entry.client-ABC123.js", "remix", None),
            ("https://example.com/sites/default/files/js/js_abc.js", "drupal", None),
            ("https://example.com/static/version1700000000/frontend/Magento/luma/en_US/requirejs/require.js", "magento", None),
            ("https://cdn.shopify.com/s/files/1/theme.js", "shopify", None),
            ("https://static.parastorage.com/services/wix-thunderbolt/dist/main.js", "wix", None),
            ("https://assets.squarespace.com/universal/scripts-compressed/common.js", "squarespace", None),
            ("https://example.com/livewire/livewire.min.js", "livewire", None),
            ("https://www.googletagmanager.com/gtm.js", "google_tag_manager", None),
            ("https://www.google-analytics.com/analytics.js", "google_analytics", None),
        ];
        for (url, id, version) in samples {
            assert!(
                found(detect_url(url)).contains(&one(id, *version)),
                "{id} {version:?} in {url}: {:?}",
                found(detect_url(url))
            );
        }
        assert_eq!(
            found(detect_url(
                "https://code.example.com/ui/1.13.2/jquery-ui.js"
            )),
            [one("jquery_ui", Some("1.13.2"))],
            "jQuery UI's own URL names only jQuery UI"
        );
        for url in [
            "https://example.com/js/app.min.js",
            "https://cdn.example.com/npm/left-pad@1.3.0/index.js",
            "https://example.com/images/jquery-logo.png",
            "https://example.com/users/jquery@example.com/profile",
        ] {
            assert!(detect_url(url).is_empty(), "{url}");
        }
    }

    #[test]
    fn a_named_version_fills_a_product_found_without_one() {
        let mut found = Vec::new();
        merge(&mut found, known("vue", None).unwrap());
        merge(&mut found, known("vue", Some("3.4.21")).unwrap());
        merge(&mut found, known("vue", Some("2.7.16")).unwrap());
        assert_eq!(
            found
                .iter()
                .map(|item| item.version.as_deref())
                .collect::<Vec<_>>(),
            [Some("3.4.21")]
        );
        assert!(known("vue", Some("<b>")).unwrap().version.is_none());
    }
}
