//! What a response says about the servers behind its origin: the edge network
//! or WAF in front, the load balancer or gateway, the hosting platform, the web
//! and application servers, the backend framework, its language and database,
//! and what it exposes by mistake.
//!
//! Four places say it, read without sending anything:
//! - the names of vendor headers (`CF-RAY`, `X-Amz-Cf-Id`, `x-envoy-*`);
//! - header values the frozen header catalog does not name (`Server: AmazonS3`,
//!   `Via: 1.1 … (CloudFront)`, `X-AspNet-Version: 4.0.30319`);
//! - the names of cookies a response sets (`JSESSIONID`, `AWSALB`,
//!   `BIGipServer…`), never their values except to tell that a BIG-IP cookie
//!   encodes a backend address;
//! - error and default pages, debug pages, stack traces and SQL errors in the
//!   bodies the map already reads.
//!
//! ZAP's passive rules, wafw00f's WAF checks and nuclei's technology templates
//! were the checklist; every rule here is written for this table.
use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::{LazyLock, Mutex},
};

use regex::{Regex, RegexSet};

use crate::service_map_libraries::Detection;

pub const EDGE: &str = "edge";
pub const GATEWAY: &str = "gateway";
pub const PLATFORM: &str = "platform";
pub const WEB_SERVER: &str = "web_server";
pub const APP_SERVER: &str = "app_server";
pub const BACKEND: &str = "backend";
pub const LANGUAGE: &str = "language";
pub const DATABASE: &str = "database";
pub const OS: &str = "os";
pub const EXPOSURE: &str = "exposure";
/// Categories in the order the inspector lists them, outermost layer first.
pub const CATEGORIES: [&str; 10] = [
    EXPOSURE, EDGE, GATEWAY, PLATFORM, WEB_SERVER, APP_SERVER, BACKEND, LANGUAGE, DATABASE, OS,
];
/// Where a server-side detection was seen. An asset's front-end detections
/// may belong to the page that loaded it; these always belong to the origin
/// that responded.
pub const SIGNALS: [&str; 3] = ["header", "cookie", "body"];

const PRODUCTS: &[(&str, &str, &str)] = &[
    ("cloudflare", "Cloudflare", EDGE),
    ("amazon_cloudfront", "Amazon CloudFront", EDGE),
    ("akamai", "Akamai", EDGE),
    ("fastly", "Fastly", EDGE),
    ("azure_front_door", "Azure Front Door", EDGE),
    ("imperva", "Imperva", EDGE),
    ("sucuri", "Sucuri", EDGE),
    ("modsecurity", "ModSecurity", EDGE),
    ("f5_advanced_waf", "F5 Advanced WAF", EDGE),
    ("barracuda", "Barracuda WAF", EDGE),
    ("fortiweb", "FortiWeb", EDGE),
    ("aws_alb", "AWS Application Load Balancer", GATEWAY),
    ("aws_elb", "AWS Classic Load Balancer", GATEWAY),
    ("aws_api_gateway", "Amazon API Gateway", GATEWAY),
    ("f5_big_ip", "F5 BIG-IP", GATEWAY),
    ("citrix_adc", "Citrix ADC", GATEWAY),
    ("haproxy", "HAProxy", GATEWAY),
    ("envoy", "Envoy", GATEWAY),
    ("istio", "Istio", GATEWAY),
    ("kong", "Kong", GATEWAY),
    ("varnish", "Varnish", GATEWAY),
    ("squid", "Squid", GATEWAY),
    ("ingress_nginx", "ingress-nginx", GATEWAY),
    ("iis_arr", "IIS Application Request Routing", GATEWAY),
    ("amazon_s3", "Amazon S3", PLATFORM),
    ("google_cloud", "Google Cloud", PLATFORM),
    ("google_cloud_storage", "Google Cloud Storage", PLATFORM),
    ("azure_storage", "Azure Storage", PLATFORM),
    ("vercel", "Vercel", PLATFORM),
    ("netlify", "Netlify", PLATFORM),
    ("fly_io", "Fly.io", PLATFORM),
    ("github", "GitHub", PLATFORM),
    ("heroku", "Heroku", PLATFORM),
    ("render", "Render", PLATFORM),
    ("cloud_foundry", "Cloud Foundry", PLATFORM),
    ("plesk", "Plesk", PLATFORM),
    ("nginx", "nginx", WEB_SERVER),
    ("apache_http_server", "Apache", WEB_SERVER),
    ("microsoft_iis", "Microsoft IIS", WEB_SERVER),
    ("microsoft_httpapi", "Microsoft HTTP.sys", WEB_SERVER),
    ("litespeed", "LiteSpeed", WEB_SERVER),
    ("openresty", "OpenResty", WEB_SERVER),
    ("tengine", "Tengine", WEB_SERVER),
    ("lighttpd", "lighttpd", WEB_SERVER),
    ("ibm_http_server", "IBM HTTP Server", WEB_SERVER),
    ("oracle_http_server", "Oracle HTTP Server", WEB_SERVER),
    ("apache_tomcat", "Apache Tomcat", APP_SERVER),
    ("jetty", "Jetty", APP_SERVER),
    ("undertow", "Undertow", APP_SERVER),
    ("wildfly", "WildFly", APP_SERVER),
    ("payara", "Payara Server", APP_SERVER),
    ("glassfish", "GlassFish", APP_SERVER),
    ("open_liberty", "Open Liberty", APP_SERVER),
    ("jboss", "JBoss", APP_SERVER),
    ("oracle_weblogic", "Oracle WebLogic", APP_SERVER),
    ("websphere", "IBM WebSphere", APP_SERVER),
    ("java_servlet", "Java Servlet", APP_SERVER),
    ("kestrel", "Kestrel", APP_SERVER),
    ("werkzeug", "Werkzeug", APP_SERVER),
    ("tornado", "Tornado", APP_SERVER),
    ("hypercorn", "Hypercorn", APP_SERVER),
    ("daphne", "Daphne", APP_SERVER),
    ("cherrypy", "CherryPy", APP_SERVER),
    ("passenger", "Phusion Passenger", APP_SERVER),
    ("webrick", "WEBrick", APP_SERVER),
    ("cowboy", "Cowboy", APP_SERVER),
    ("spring_boot", "Spring Boot", BACKEND),
    ("express", "Express", BACKEND),
    ("django", "Django", BACKEND),
    ("django_rest_framework", "Django REST framework", BACKEND),
    ("fastapi", "FastAPI", BACKEND),
    ("flask", "Flask", BACKEND),
    ("laravel", "Laravel", BACKEND),
    ("symfony", "Symfony", BACKEND),
    ("codeigniter", "CodeIgniter", BACKEND),
    ("cakephp", "CakePHP", BACKEND),
    ("ruby_on_rails", "Ruby on Rails", BACKEND),
    ("asp_net", "ASP.NET", BACKEND),
    ("asp_net_mvc", "ASP.NET MVC", BACKEND),
    ("asp_net_core", "ASP.NET Core", BACKEND),
    ("asp_classic", "Classic ASP", BACKEND),
    ("play_framework", "Play Framework", BACKEND),
    ("sails", "Sails", BACKEND),
    ("keycloak", "Keycloak", BACKEND),
    ("apollo_server", "Apollo Server", BACKEND),
    ("jenkins", "Jenkins", BACKEND),
    ("gitlab", "GitLab", BACKEND),
    ("atlassian", "Atlassian", BACKEND),
    ("kibana", "Kibana", BACKEND),
    ("grafana", "Grafana", BACKEND),
    ("jfrog_artifactory", "JFrog Artifactory", BACKEND),
    ("php", "PHP", LANGUAGE),
    ("java", "Java", LANGUAGE),
    ("python", "Python", LANGUAGE),
    ("node_js", "Node.js", LANGUAGE),
    ("ruby", "Ruby", LANGUAGE),
    ("dotnet", ".NET", LANGUAGE),
    ("go", "Go", LANGUAGE),
    ("deno", "Deno", LANGUAGE),
    ("mysql", "MySQL", DATABASE),
    ("postgresql", "PostgreSQL", DATABASE),
    ("oracle_database", "Oracle Database", DATABASE),
    ("sql_server", "SQL Server", DATABASE),
    ("sqlite", "SQLite", DATABASE),
    ("mongodb", "MongoDB", DATABASE),
    ("redis", "Redis", DATABASE),
    ("elasticsearch", "Elasticsearch", DATABASE),
    ("ubuntu", "Ubuntu", OS),
    ("debian", "Debian", OS),
    ("centos", "CentOS", OS),
    ("red_hat", "Red Hat", OS),
    ("windows", "Windows", OS),
    ("freebsd", "FreeBSD", OS),
    ("unix", "Unix", OS),
    ("django_debug", "Django debug page", EXPOSURE),
    ("werkzeug_debugger", "Werkzeug debugger", EXPOSURE),
    ("laravel_debug", "Laravel debug page", EXPOSURE),
    ("symfony_debug", "Symfony debug page", EXPOSURE),
    ("symfony_profiler", "Symfony profiler", EXPOSURE),
    ("rails_debug", "Rails debug page", EXPOSURE),
    ("aspnet_error", "ASP.NET detailed error", EXPOSURE),
    ("iis_error", "IIS detailed error", EXPOSURE),
    ("php_error", "PHP error message", EXPOSURE),
    ("phpinfo", "phpinfo() page", EXPOSURE),
    ("java_stack_trace", "Java stack trace", EXPOSURE),
    ("dotnet_stack_trace", ".NET stack trace", EXPOSURE),
    ("python_traceback", "Python traceback", EXPOSURE),
    ("node_stack_trace", "Node.js stack trace", EXPOSURE),
    ("go_panic", "Go panic trace", EXPOSURE),
    ("sql_error", "SQL error message", EXPOSURE),
    ("directory_listing", "Directory listing", EXPOSURE),
    ("spring_actuator", "Spring Boot Actuator", EXPOSURE),
    ("api_docs", "API documentation", EXPOSURE),
    (
        "elasticsearch_root",
        "Elasticsearch root endpoint",
        EXPOSURE,
    ),
    (
        "backend_address",
        "Backend address in a BIG-IP cookie",
        EXPOSURE,
    ),
    ("backend_host_header", "Backend host header", EXPOSURE),
    ("source_map", "Source map reference", EXPOSURE),
    ("aws_access_key", "AWS access key ID", EXPOSURE),
    ("google_api_key", "Google API key", EXPOSURE),
    ("slack_token", "Slack token", EXPOSURE),
    ("slack_webhook", "Slack webhook URL", EXPOSURE),
    ("github_token", "GitHub token", EXPOSURE),
    ("stripe_secret_key", "Stripe secret key", EXPOSURE),
    ("sendgrid_key", "SendGrid API key", EXPOSURE),
    ("private_key", "Private key", EXPOSURE),
    ("jwt", "JSON Web Token", EXPOSURE),
    ("private_ip", "Private IP address", EXPOSURE),
];

// (header name, value rule, product). A name ending in `*` is a prefix; an
// empty value rule takes any value. `§` in a value rule is the version.
const HEADER_RULES: &[(&str, &str, &str)] = &[
    ("cf-ray", "", "cloudflare"),
    ("cf-cache-status", "", "cloudflare"),
    ("x-amz-cf-id", "", "amazon_cloudfront"),
    ("x-amz-cf-pop", "", "amazon_cloudfront"),
    ("x-cache", r"(?i)\bcloudfront\b", "amazon_cloudfront"),
    ("via", r"(?i)\(cloudfront\)", "amazon_cloudfront"),
    ("x-fastly-request-id", "", "fastly"),
    ("x-served-by", r"^cache-[a-z0-9]+-[A-Z]{3}", "fastly"),
    ("akamai-grn", "", "akamai"),
    ("x-akamai-transformed", "", "akamai"),
    ("x-akamai-request-id", "", "akamai"),
    ("x-azure-ref", "", "azure_front_door"),
    ("x-iinfo", "", "imperva"),
    ("x-cdn", r"(?i)imperva|incapsula", "imperva"),
    ("x-sucuri-id", "", "sucuri"),
    ("x-sucuri-cache", "", "sucuri"),
    ("x-amz-apigw-id", "", "aws_api_gateway"),
    ("x-amzn-errortype", "", "aws_api_gateway"),
    ("x-amz-id-2", "", "amazon_s3"),
    ("x-goog-generation", "", "google_cloud_storage"),
    ("x-guploader-uploadid", "", "google_cloud_storage"),
    ("x-ms-blob-type", "", "azure_storage"),
    ("x-vercel-id", "", "vercel"),
    ("x-vercel-cache", "", "vercel"),
    ("x-nf-request-id", "", "netlify"),
    ("fly-request-id", "", "fly_io"),
    ("x-github-request-id", "", "github"),
    ("via", r"(?i)\bvegur\b", "heroku"),
    ("via", r"(?i)^1\.1 google$", "google_cloud"),
    ("x-render-origin-server", "", "render"),
    ("rndr-id", "", "render"),
    ("x-envoy-*", "", "envoy"),
    ("x-kong-*", "", "kong"),
    ("via", r"(?i)\bkong/§", "kong"),
    ("x-varnish", "", "varnish"),
    ("via", r"(?i)\bvarnish\b", "varnish"),
    ("via", r"\bNS-CACHE", "citrix_adc"),
    ("x-runtime", r"^[0-9]+(?:\.[0-9]+)?$", "ruby"),
    ("x-debug-token", "", "symfony"),
    ("x-debug-token", "", "symfony_profiler"),
    ("x-debug-token-link", "", "symfony_profiler"),
    ("x-aspnet-version", "^§", "asp_net"),
    ("x-aspnetmvc-version", "^§", "asp_net_mvc"),
    ("x-drupal-cache", "", "drupal"),
    ("x-drupal-dynamic-cache", "", "drupal"),
    ("x-litespeed-cache", "", "litespeed"),
    ("x-turbo-charged-by", r"(?i)litespeed", "litespeed"),
    ("x-pingback", r"/xmlrpc\.php$", "wordpress"),
    ("x-shopify-stage", "", "shopify"),
    ("x-shopid", "", "shopify"),
    ("x-wix-request-id", "", "wix"),
    ("x-magento-tags", "", "magento"),
    ("x-magento-cache-debug", "", "magento"),
    ("x-oracle-dms-ecid", "", "oracle_weblogic"),
    ("x-jenkins", "^§", "jenkins"),
    ("x-hudson", "", "jenkins"),
    ("x-gitlab-meta", "", "gitlab"),
    ("x-arequestid", "", "atlassian"),
    ("kbn-name", "", "kibana"),
    ("kbn-version", "^§", "kibana"),
    ("x-elastic-product", r"(?i)^elasticsearch$", "elasticsearch"),
    (
        "x-jfrog-version",
        r"(?i)\bartifactory/§",
        "jfrog_artifactory",
    ),
    ("x-powered-by-plesk", "", "plesk"),
    ("x-backend-server", "", "backend_host_header"),
    ("x-backend-host", "", "backend_host_header"),
    ("x-upstream", "", "backend_host_header"),
    ("sourcemap", "", "source_map"),
    ("x-sourcemap", "", "source_map"),
    // Server and X-Powered-By products the frozen header catalog does not name.
    ("server", r"(?i)^AmazonS3$", "amazon_s3"),
    ("server", r"(?i)^awselb(?:/§)?", "aws_elb"),
    ("server", r"(?i)^Google Frontend$", "google_cloud"),
    ("server", r"(?i)^Akamai(?:GHost|NetStorage)\b", "akamai"),
    ("server", r"(?i)^CloudFront$", "amazon_cloudfront"),
    ("server", r"(?i)^Kestrel$", "kestrel"),
    ("server", r"(?i)^Microsoft-HTTPAPI/§", "microsoft_httpapi"),
    ("server", r"(?i)^BIG-?IP$", "f5_big_ip"),
    ("server", r"(?i)^Netlify$", "netlify"),
    ("server", r"(?i)^Vercel$", "vercel"),
    ("server", r"(?i)^GitHub\.com$", "github"),
    ("server", r"(?i)^Tengine(?:/§)?", "tengine"),
    ("server", r"(?i)^lighttpd(?:/§)?", "lighttpd"),
    ("server", r"(?i)^Werkzeug/§", "werkzeug"),
    ("server", r"(?i)\bPython/§", "python"),
    ("server", r"(?i)^TornadoServer/§", "tornado"),
    ("server", r"(?i)^Cowboy$", "cowboy"),
    (
        "server",
        r"(?i)\bPhusion[_ ]Passenger(?:[ /]§)?",
        "passenger",
    ),
    ("server", r"(?i)^WEBrick/§", "webrick"),
    ("server", r"(?i)\bRuby/§", "ruby"),
    ("server", r"(?i)^Undertow(?:/§)?", "undertow"),
    ("server", r"(?i)^WildFly/§", "wildfly"),
    ("server", r"(?i)^Payara Server(?:\s+§)?", "payara"),
    (
        "server",
        r"(?i)^GlassFish Server(?: Open Source Edition)?(?:\s+§)?",
        "glassfish",
    ),
    ("server", r"(?i)^JBoss", "jboss"),
    ("server", r"(?i)^Oracle-HTTP-Server", "oracle_http_server"),
    ("server", r"(?i)^WebLogic", "oracle_weblogic"),
    ("server", r"(?i)^IBM_HTTP_Server", "ibm_http_server"),
    ("server", r"(?i)^WebSphere", "websphere"),
    ("server", r"(?i)^squid(?:/§)?", "squid"),
    ("server", r"(?i)^Varnish", "varnish"),
    ("server", r"(?i)^istio-envoy$", "istio"),
    ("server", r"(?i)^kong(?:/§)?", "kong"),
    ("server", r"(?i)^Sucuri", "sucuri"),
    ("server", r"(?i)mod_security|^NOYB$", "modsecurity"),
    ("server", r"(?i)^hypercorn", "hypercorn"),
    ("server", r"(?i)^daphne", "daphne"),
    ("server", r"(?i)^CherryPy/§", "cherrypy"),
    ("server", r"(?i)^Deno/§", "deno"),
    ("server", r"\(Ubuntu\)", "ubuntu"),
    ("server", r"\(Debian\)", "debian"),
    ("server", r"\(CentOS\)", "centos"),
    ("server", r"\(Red Hat(?: Enterprise Linux)?\)", "red_hat"),
    ("server", r"\(Win(?:32|64)\)", "windows"),
    ("server", r"\(FreeBSD\)", "freebsd"),
    ("server", r"\(Unix\)", "unix"),
    ("x-powered-by", r"(?i)\bServlet/§", "java_servlet"),
    ("x-powered-by", r"(?i)\bJSP/", "java"),
    ("x-powered-by", r"(?i)\bUndertow(?:/§)?", "undertow"),
    ("x-powered-by", r"(?i)\bWildFly/§", "wildfly"),
    ("x-powered-by", r"(?i)\bPayara Server(?:\s+§)?", "payara"),
    (
        "x-powered-by",
        r"(?i)\bGlassFish Server(?: Open Source Edition)?(?:\s+§)?",
        "glassfish",
    ),
    ("x-powered-by", r"(?i)\bJBoss", "jboss"),
    (
        "x-powered-by",
        r"(?i)\bPhusion Passenger(?: §)?",
        "passenger",
    ),
    ("x-powered-by", r"(?i)^Plesk", "plesk"),
    ("x-powered-by", r"(?i)\bARR/§", "iis_arr"),
    ("x-powered-by", r"(?i)^Sails\b", "sails"),
    ("x-powered-by", r"(?i)\bMono\b", "dotnet"),
];

// Set-Cookie names. A value is never read, except a BIG-IP cookie's below.
const COOKIE_RULES: &[(&str, &str)] = &[
    (r"^JSESSIONID$", "java_servlet"),
    (r"^PHPSESSID$", "php"),
    (r"^ASP\.NET_SessionId$", "asp_net"),
    (r"^ASPSESSIONID[A-Z]{8}$", "asp_classic"),
    (r"^\.AspNetCore\.", "asp_net_core"),
    (r"^laravel_session$", "laravel"),
    (r"^ci_session$", "codeigniter"),
    (r"^CAKEPHP$", "cakephp"),
    (r"^PLAY_SESSION$", "play_framework"),
    (r"^connect\.sid$", "express"),
    (r"^csrftoken$", "django"),
    (r"^__VCAP_ID__$", "cloud_foundry"),
    (r"^AWSALB(?:CORS|TG|TGCORS)?$", "aws_alb"),
    (r"^AWSELB(?:CORS)?$", "aws_elb"),
    (r"^BIGipServer", "f5_big_ip"),
    (r"^TS[0-9a-f]{6}(?:[0-9a-f]{2})?$", "f5_advanced_waf"),
    (
        r"^(?:__cfduid|__cf_bm|cf_clearance|__cflb|__cfruid|_cfuvid)$",
        "cloudflare",
    ),
    (r"^(?:incap_ses_|visid_incap_|nlbi_)", "imperva"),
    (r"^(?:ak_bmsc|bm_sz|_abck|bm_sv|bm_mi)$", "akamai"),
    (r"^(?:ns_af|citrix_ns_id|NSC_)", "citrix_adc"),
    (
        r"^(?:barra_counter_session|BNI__BARRACUDA_LB_COOKIE|BNI_persistence)$",
        "barracuda",
    ),
    (r"^FORTIWAFSID$", "fortiweb"),
    (
        r"^(?:KEYCLOAK_SESSION|KEYCLOAK_IDENTITY|KC_RESTART|AUTH_SESSION_ID)(?:_LEGACY)?$",
        "keycloak",
    ),
    (
        r"^(?:wordpress_logged_in_|wordpress_sec_|wp-settings-)",
        "wordpress",
    ),
    (r"^S?SESS[0-9a-f]{32}$", "drupal"),
    (r"^mage-cache-sessid$", "magento"),
    (r"^(?:_shopify_y|_shopify_s|cart_sig)$", "shopify"),
];

// What a body says. Pages and error responses the map already reads.
const BODY_RULES: &[(&str, &str)] = &[
    ("nginx", r"<hr><center>nginx(?:/§)?</center>"),
    ("openresty", r"<hr><center>openresty(?:/§)?</center>"),
    ("tengine", r"Powered by Tengine(?:/§)?"),
    (
        "apache_http_server",
        r"<address>Apache(?:/§)?[^<]{0,200} Server at [^<]{1,200} Port [0-9]+</address>",
    ),
    ("ubuntu", r"<address>Apache/[^ <]+ \(Ubuntu\)"),
    ("debian", r"<address>Apache/[^ <]+ \(Debian\)"),
    ("centos", r"<address>Apache/[^ <]+ \(CentOS\)"),
    ("red_hat", r"<address>Apache/[^ <]+ \(Red Hat"),
    ("windows", r"<address>Apache/[^ <]+ \(Win(?:32|64)\)"),
    ("unix", r"<address>Apache/[^ <]+ \(Unix\)"),
    ("directory_listing", r"<title>Index of /[^<]{0,300}</title>"),
    ("microsoft_iis", r"<title>IIS §\s*Detailed Error"),
    ("iis_error", r"<title>IIS [0-9.]+\s*Detailed Error"),
    ("microsoft_iis", r"<title>IIS Windows Server</title>"),
    ("apache_tomcat", r"Apache Tomcat/§"),
    ("jetty", r"Powered by Jetty:// §"),
    ("jboss", r"\bJBWEB[0-9]{6}\b"),
    ("wildfly", r"<title>Welcome to WildFly</title>"),
    // Undertow's whole default error page.
    (
        "undertow",
        r"\A<html><head><title>Error</title></head><body>(?:[0-9]{3} - )?[A-Za-z ]{2,60}</body></html>\s*\z",
    ),
    (
        "open_liberty",
        r"<title>Open Liberty</title>|Property of the Open Liberty project",
    ),
    // The servlet engine's own message codes, in WebSphere and Liberty.
    ("websphere", r"\bSRVE0(?:190|207|232|255|295)E\b"),
    ("payara", r"<title>Payara Server ([0-9]+(?:\.[0-9]+)*)"),
    (
        "glassfish",
        r"<title>GlassFish Server Open Source Edition\s+([0-9]+(?:\.[0-9]+)*)\s*-\s*Error report</title>",
    ),
    ("grafana", r"window\.grafanaBootData"),
    ("grafana", r#""buildInfo":\{[^{}]{0,400}?"version":"§""#),
    ("spring_boot", r"<h1>Whitelabel Error Page</h1>"),
    (
        "spring_boot",
        r#"\{"timestamp":"[^"]{4,40}","status":[0-9]{3},"error":"[^"]{1,80}"[^{}]{0,2000}?"path":""#,
    ),
    (
        "spring_boot",
        r#""_links"\s*:\s*\{\s*"self"\s*:\s*\{\s*"href"\s*:\s*"[^"]*/actuator""#,
    ),
    (
        "spring_actuator",
        r#""_links"\s*:\s*\{\s*"self"\s*:\s*\{\s*"href"\s*:\s*"[^"]*/actuator""#,
    ),
    (
        "express",
        r"<pre>Cannot (?:GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS) /[^<]{0,300}</pre>",
    ),
    (
        "django",
        r"You(?:'|’|&#x27;|&#39;|&rsquo;)re seeing this error because you have <code>DEBUG = True</code>",
    ),
    (
        "django_debug",
        r"You(?:'|’|&#x27;|&#39;|&rsquo;)re seeing this error because you have <code>DEBUG = True</code>",
    ),
    (
        "django",
        r"<p>CSRF verification failed\. Request aborted\.</p>",
    ),
    (
        "django_rest_framework",
        r#"^\{"detail":"(?:Not found\.|Authentication credentials were not provided\.|You do not have permission to perform this action\.|Method \\"[A-Z]+\\" not allowed\.)"\}$"#,
    ),
    (
        "fastapi",
        r#"^\{"detail":"(?:Not Found|Method Not Allowed)"\}$"#,
    ),
    ("fastapi", r#"^\{"detail":\[\{"(?:type|loc)":"#),
    // Werkzeug's own not-found text, which a Flask app serves whatever server
    // runs it; it does not show the development server.
    (
        "flask",
        r"The requested URL was not found on the server\. If you entered the URL manually please check your spelling and try again\.",
    ),
    (
        "werkzeug",
        r"The debugger caught an exception in your WSGI application",
    ),
    (
        "werkzeug_debugger",
        r"The debugger caught an exception in your WSGI application",
    ),
    (
        "laravel",
        r"window\.ignite|Illuminate\\{1,2}(?:Foundation|Routing|Database)\\",
    ),
    (
        "laravel_debug",
        r"window\.ignite|Illuminate\\{1,2}(?:Foundation|Routing|Database)\\",
    ),
    (
        "symfony",
        r"<title>Symfony Exception</title>|\bclass=.sf-toolbar",
    ),
    (
        "symfony_debug",
        r"<title>Symfony Exception</title>|\bclass=.sf-toolbar",
    ),
    (
        "ruby_on_rails",
        r"<title>Action Controller: Exception caught</title>",
    ),
    (
        "rails_debug",
        r"<title>Action Controller: Exception caught</title>",
    ),
    (
        "ruby_on_rails",
        r"The page you were looking for doesn(?:'|&#39;)t exist\.|The change you wanted was rejected\.",
    ),
    ("asp_net", r"Server Error in '[^']{0,200}' Application\."),
    (
        "aspnet_error",
        r"Server Error in '[^']{0,200}' Application\.",
    ),
    ("asp_net", r"ASP\.NET Version:§"),
    ("dotnet", r"Microsoft \.NET Framework Version:§"),
    (
        "php",
        r"<b>(?:Fatal error|Parse error|Warning|Notice|Deprecated)</b>:\s[^\n]{0,500}? on line <b>[0-9]+</b>",
    ),
    (
        "php_error",
        r"<b>(?:Fatal error|Parse error|Warning|Notice|Deprecated)</b>:\s[^\n]{0,500}? on line <b>[0-9]+</b>",
    ),
    ("php", r"<title>PHP § - phpinfo\(\)</title>"),
    ("phpinfo", r"<title>PHP [^<]{0,20}phpinfo\(\)</title>"),
    (
        "java",
        r"\bat (?:java|javax|jakarta|sun|org\.springframework|org\.apache|org\.hibernate|com\.sun)\.[\w.$]+\([\w$]*\.java:[0-9]+\)",
    ),
    (
        "java_stack_trace",
        r"\bat (?:java|javax|jakarta|sun|org\.springframework|org\.apache|org\.hibernate|com\.sun)\.[\w.$]+\([\w$]*\.java:[0-9]+\)",
    ),
    (
        "dotnet",
        r"\bat (?:System|Microsoft)\.[\w.`]+\([^)\n]{0,300}\) in [^\n<]{1,200}:line [0-9]+|--- End of stack trace from previous location",
    ),
    (
        "dotnet_stack_trace",
        r"\bat (?:System|Microsoft)\.[\w.`]+\([^)\n]{0,300}\) in [^\n<]{1,200}:line [0-9]+|--- End of stack trace from previous location",
    ),
    ("python", r"Traceback \(most recent call last\):"),
    ("python_traceback", r"Traceback \(most recent call last\):"),
    (
        "node_js",
        r"\bat (?:Object\.<anonymous>|Module\._compile|Layer\.handle \[as handle_request\]|processTicksAndRejections) \(|\(node:internal/",
    ),
    (
        "node_stack_trace",
        r"\bat (?:Object\.<anonymous>|Module\._compile|Layer\.handle \[as handle_request\]|processTicksAndRejections) \(|\(node:internal/",
    ),
    ("go", r"goroutine [0-9]+ \[running\]:"),
    ("go_panic", r"goroutine [0-9]+ \[running\]:"),
    ("go", r"^404 page not found\n?$"),
    (
        "haproxy",
        r"No server is available to handle this request\.",
    ),
    (
        "envoy",
        r"upstream connect error or disconnect/reset before headers|^no healthy upstream$",
    ),
    ("varnish", r"Guru Meditation:"),
    ("squid", r"\(squid/§\)"),
    ("cloudflare", r"cf-error-details|Cloudflare Ray ID"),
    (
        "akamai",
        r"Reference(?:&#32;| )(?:&#35;|#)[0-9]+(?:\.|&#46;)[0-9a-f]+(?:\.|&#46;)[0-9]+(?:\.|&#46;)[0-9a-f]+",
    ),
    (
        "amazon_cloudfront",
        r"Generated by cloudfront \(CloudFront\)",
    ),
    ("imperva", r"Incapsula incident ID|/_Incapsula_Resource"),
    ("sucuri", r"Sucuri WebSite Firewall"),
    (
        "modsecurity",
        r"This error was generated by Mod.?Security|Protected by Mod.?Security",
    ),
    (
        "kong",
        r#""message"\s*:\s*"no Route matched with those values""#,
    ),
    (
        "aws_api_gateway",
        r#"^\{"message":"Missing Authentication Token"\}$"#,
    ),
    ("ingress_nginx", r"default backend - 404"),
    (
        "elasticsearch",
        r#""number"\s*:\s*"§"[\s\S]{0,800}?"tagline"\s*:\s*"You Know, for Search""#,
    ),
    (
        "elasticsearch_root",
        r#""tagline"\s*:\s*"You Know, for Search""#,
    ),
    (
        "apollo_server",
        r#""code"\s*:\s*"GRAPHQL_(?:VALIDATION|PARSE)_FAILED""#,
    ),
    (
        "api_docs",
        r#"<title>Swagger UI</title>|swagger-ui-bundle\.js|^\{\s*"(?:openapi|swagger)"\s*:\s*"[23]\."#,
    ),
    (
        "mysql",
        r"You have an error in your SQL syntax|\bmysqli?_[a-z_]+\(\)|com\.mysql\.(?:cj\.)?jdbc|MySQLSyntaxErrorException",
    ),
    (
        "postgresql",
        r"\bPSQLException\b|\bpg_query\(\)|ERROR:\s+syntax error at or near|org\.postgresql\.util",
    ),
    ("oracle_database", r"\bORA-[0-9]{5}\b|oracle\.jdbc\.driver"),
    (
        "sql_server",
        r"Unclosed quotation mark after the character string|Microsoft OLE DB Provider for SQL Server|(?:System|Microsoft)\.Data\.SqlClient\.SqlException",
    ),
    (
        "sqlite",
        r"\bSQLite3::|sqlite3\.OperationalError|\bSQLITE_ERROR\b",
    ),
    (
        "mongodb",
        r"\bMongo(?:Server)?Error\b|E11000 duplicate key error",
    ),
    (
        "redis",
        r"WRONGTYPE Operation against a key holding the wrong kind of value",
    ),
    (
        "sql_error",
        r"You have an error in your SQL syntax|ERROR:\s+syntax error at or near|\bORA-[0-9]{5}\b|Unclosed quotation mark after the character string|sqlite3\.OperationalError|\bSQLITE_ERROR\b",
    ),
];

// What a script, page or error body gives away by mistake: where its source
// map is, a credential written into it, an internal address. Only the finding
// is reported, never the matched value; View source opens the row.
const EXPOSURE_RULES: &[(&str, &str)] = &[
    ("source_map", r#"//[#@] ?sourceMappingURL=[^\s'"]"#),
    ("aws_access_key", r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
    (
        "google_api_key",
        r"\bAIza[0-9A-Za-z_-]{35}(?:[^0-9A-Za-z_-]|$)",
    ),
    ("slack_token", r"\bxox[baprs]-[0-9A-Za-z-]{10,}"),
    (
        "slack_webhook",
        r"https://hooks\.slack\.com/services/T[0-9A-Z]+/B[0-9A-Z]+/[0-9A-Za-z]+",
    ),
    ("github_token", r"\bgh[pousr]_[0-9A-Za-z]{36}\b"),
    ("stripe_secret_key", r"\b[rs]k_live_[0-9A-Za-z]{24,}"),
    (
        "sendgrid_key",
        r"\bSG\.[0-9A-Za-z_-]{22}\.[0-9A-Za-z_-]{43}\b",
    ),
    // Only with key material after the header: a PEM parser in a script
    // carries the bare header as a constant.
    (
        "private_key",
        r"-----BEGIN (?:RSA |EC |DSA |OPENSSH |ENCRYPTED |PGP )?PRIVATE KEY(?: BLOCK)?-----(?:\\[rn]|\s)*[A-Za-z0-9+/]{40}",
    ),
    (
        "jwt",
        r"\beyJ[0-9A-Za-z_-]{10,}\.eyJ[0-9A-Za-z_-]{10,}\.[0-9A-Za-z_-]{10,}",
    ),
];
// A private address in a URL a script calls (`http://10.1.2.3:8080/api`), or
// anywhere in an error. A page's prose about routers is not read for these.
const PRIVATE_URL: &str = r"(?:https?:)?//(?:10\.[0-9]{1,3}|172\.(?:1[6-9]|2[0-9]|3[01])|192\.168)\.[0-9]{1,3}\.[0-9]{1,3}\b";
const PRIVATE_IP: &str = r"(?:^|[^0-9A-Za-z.])(?:10\.[0-9]{1,3}|172\.(?:1[6-9]|2[0-9]|3[01])|192\.168)\.[0-9]{1,3}\.[0-9]{1,3}(?:[^0-9A-Za-z.]|$)";

const VERSION: &str = r"([0-9][0-9A-Za-z.+-]{0,39})";

struct Rules {
    set: RegexSet,
    each: Vec<Regex>,
}

fn compile<'a>(patterns: impl Iterator<Item = &'a str>) -> Rules {
    let patterns: Vec<String> = patterns
        .map(|pattern| pattern.replace('§', VERSION))
        .collect();
    Rules {
        set: RegexSet::new(&patterns).expect("valid server rules"),
        each: patterns
            .iter()
            .map(|pattern| Regex::new(pattern).expect("valid server rule"))
            .collect(),
    }
}

/// The header rules by name, so a value is matched only against its own
/// header's rules, and a header no rule names costs one lookup.
struct HeaderRules {
    exact: HashMap<&'static str, Vec<usize>>,
    prefixes: Vec<(&'static str, usize)>,
    /// Per header name, the rules that read its value, compiled together.
    values: HashMap<&'static str, (Rules, Vec<usize>)>,
}

static HEADERS: LazyLock<HeaderRules> = LazyLock::new(|| {
    let mut exact: HashMap<&'static str, Vec<usize>> = HashMap::new();
    let mut prefixes = Vec::new();
    let mut valued: HashMap<&'static str, Vec<usize>> = HashMap::new();
    for (index, (name, value, _)) in HEADER_RULES.iter().enumerate() {
        match name.strip_suffix('*') {
            Some(prefix) => prefixes.push((prefix, index)),
            None => exact.entry(*name).or_default().push(index),
        }
        if !value.is_empty() {
            valued.entry(*name).or_default().push(index);
        }
    }
    let values = valued
        .into_iter()
        .map(|(name, indexes)| {
            let rules = compile(indexes.iter().map(|index| HEADER_RULES[*index].1));
            (name, (rules, indexes))
        })
        .collect();
    HeaderRules {
        exact,
        prefixes,
        values,
    }
});
// A session repeats a few Server, Via and Set-Cookie values on every row; each
// is matched once.
static SEEN: LazyLock<Mutex<HashMap<u64, Vec<Detection>>>> = LazyLock::new(Default::default);
static COOKIES: LazyLock<Rules> =
    LazyLock::new(|| compile(COOKIE_RULES.iter().map(|(name, _)| *name)));
static EXPOSURES: LazyLock<Rules> =
    LazyLock::new(|| compile(EXPOSURE_RULES.iter().map(|(_, pattern)| *pattern)));
static PRIVATE: LazyLock<(Regex, Regex)> = LazyLock::new(|| {
    (
        Regex::new(PRIVATE_URL).expect("valid private URL pattern"),
        Regex::new(PRIVATE_IP).expect("valid private address pattern"),
    )
});
static BODIES: LazyLock<Rules> =
    LazyLock::new(|| compile(BODY_RULES.iter().map(|(_, pattern)| *pattern)));

/// A server-side product, or failing that a front-end one a server signal can
/// name too (a WordPress cookie, a Drupal header).
fn known(id: &str, version: Option<&str>, signal: &'static str) -> Option<Detection> {
    let mut detection = match PRODUCTS.iter().find(|(known, _, _)| *known == id) {
        Some((id, name, category)) => Detection {
            product_id: (*id).into(),
            name: (*name).into(),
            category,
            version: None,
            signal,
        },
        None => crate::service_map_libraries::known(id, None)?,
    };
    detection.signal = signal;
    detection.version = version
        .filter(|version| crate::service_map_libraries::valid_version(version))
        .map(str::to_string);
    Some(detection)
}

fn version_of<'a>(rule: &Regex, text: &'a str) -> Option<&'a str> {
    rule.captures(text)
        .and_then(|captures| captures.get(1))
        .map(|version| version.as_str())
}

/// What a response's headers say: vendor header names, values the header
/// catalog does not name, and the names of the cookies it sets.
pub(crate) fn detect_headers<'a>(
    headers: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<Detection> {
    let mut found = Vec::new();
    let mut add = |detection: Option<Detection>| {
        if let Some(detection) = detection {
            crate::service_map_libraries::merge(&mut found, detection);
        }
    };
    for (name, value) in headers {
        let name = name.to_ascii_lowercase();
        let value = value.trim();
        let valued = name == "set-cookie" || HEADERS.values.contains_key(name.as_str());
        if !valued {
            // A vendor header's name is enough; most headers name nothing.
            let rules = HEADERS
                .exact
                .get(name.as_str())
                .into_iter()
                .flatten()
                .copied();
            let prefixed = HEADERS
                .prefixes
                .iter()
                .filter(|(prefix, _)| name.starts_with(prefix))
                .map(|(_, index)| *index);
            for index in rules.chain(prefixed) {
                add(known(HEADER_RULES[index].2, None, "header"));
            }
            continue;
        }
        let mut hasher = DefaultHasher::new();
        (name.as_str(), value).hash(&mut hasher);
        let key = hasher.finish();
        let cached = SEEN
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&key)
            .cloned();
        let detections = cached.unwrap_or_else(|| {
            let detections = valued_header(&name, value);
            let mut seen = SEEN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            // ponytail: dropping every entry at the cap is enough for the few
            // hundred distinct values a session sends; an LRU would keep the busiest.
            if seen.len() >= 4096 {
                seen.clear();
            }
            seen.insert(key, detections.clone());
            detections
        });
        for detection in detections {
            add(Some(detection));
        }
    }
    found
}

/// A Set-Cookie's name, or a header whose value its rules read.
fn valued_header(name: &str, value: &str) -> Vec<Detection> {
    let mut found = Vec::new();
    let mut add = |detection: Option<Detection>| {
        if let Some(detection) = detection {
            crate::service_map_libraries::merge(&mut found, detection);
        }
    };
    if name == "set-cookie" {
        let cookie = value.split(['=', ';']).next().unwrap_or("").trim();
        for index in COOKIES.set.matches(cookie).iter() {
            let id = COOKIE_RULES[index].1;
            add(known(id, None, "cookie"));
            if id == "f5_big_ip" && bigip_address(value) {
                add(known("backend_address", None, "cookie"));
            }
        }
        return found;
    }
    for index in HEADERS.exact.get(name).into_iter().flatten() {
        if HEADER_RULES[*index].1.is_empty() {
            add(known(HEADER_RULES[*index].2, None, "header"));
        }
    }
    if let Some((rules, indexes)) = HEADERS.values.get(name) {
        for matched in rules.set.matches(value).iter() {
            add(known(
                HEADER_RULES[indexes[matched]].2,
                version_of(&rules.each[matched], value),
                "header",
            ));
        }
    }
    found
}

/// Whether a BIG-IP persistence cookie's value encodes the pool member's
/// address: `1677787402.20480.0000` is an IPv4 address and port as
/// little-endian integers, `rd5o00000000000000000000ffffc0a80164o80` the same
/// in a route domain. The address itself never leaves this function.
fn bigip_address(set_cookie: &str) -> bool {
    let value = set_cookie
        .split_once('=')
        .map_or("", |(_, rest)| rest)
        .split(';')
        .next()
        .unwrap_or("")
        .trim();
    let mut parts = value.split('.');
    if let (Some(address), Some(port), Some("0000"), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    {
        return address.parse::<u32>().is_ok_and(|address| address != 0)
            && port.parse::<u16>().is_ok();
    }
    value.starts_with("rd")
        && value.split_once('o').is_some_and(|(_, rest)| {
            rest.len() >= 32 && rest[..32].bytes().all(|b| b.is_ascii_hexdigit())
        })
}

/// What a page or an error response's body says about its servers.
pub(crate) fn detect_body(body: &str) -> Vec<Detection> {
    let mut found = Vec::new();
    for index in BODIES.set.matches(body).iter() {
        if let Some(detection) = known(
            BODY_RULES[index].0,
            version_of(&BODIES.each[index], body),
            "body",
        ) {
            crate::service_map_libraries::merge(&mut found, detection);
        }
    }
    found
}

/// Where a body is, for what it may give away: a script's URLs, or any text
/// of an error, are read for private addresses; a page's prose is not.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Exposed {
    Script,
    Page,
    Error,
}

/// What a body gives away by mistake. The matched values never leave here.
pub(crate) fn detect_exposures(body: &str, kind: Exposed) -> Vec<Detection> {
    let mut found = Vec::new();
    let page = matches!(kind, Exposed::Page);
    let ids = EXPOSURES
        .set
        .matches(body)
        .into_iter()
        .map(|index| EXPOSURE_RULES[index].0)
        // A page's token is usually the visitor's own session; a script's or
        // an error's was written into it.
        .filter(|id| !(page && *id == "jwt"));
    let private = match kind {
        Exposed::Script => PRIVATE.0.is_match(body),
        Exposed::Error => PRIVATE.1.is_match(body),
        Exposed::Page => false,
    };
    for id in ids.chain(private.then_some("private_ip")) {
        if let Some(detection) = known(id, None, "body") {
            crate::service_map_libraries::merge(&mut found, detection);
        }
    }
    found
}

/// Whether a detection came from the responding origin's own headers,
/// cookies or body, so it is never given to the page that loaded the row.
pub(crate) fn server_signal(detection: &Detection) -> bool {
    SIGNALS.contains(&detection.signal)
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
        for id in HEADER_RULES
            .iter()
            .map(|(_, _, id)| *id)
            .chain(COOKIE_RULES.iter().map(|(_, id)| *id))
            .chain(BODY_RULES.iter().map(|(id, _)| *id))
        {
            assert!(known(id, None, "header").is_some(), "{id} is unknown");
        }
        for (_, _, category) in PRODUCTS {
            assert!(CATEGORIES.contains(category));
        }
        let indexed = HEADERS.exact.values().map(Vec::len).sum::<usize>() + HEADERS.prefixes.len();
        assert_eq!(indexed, HEADER_RULES.len());
        assert_eq!(COOKIES.each.len(), COOKIE_RULES.len());
        assert_eq!(BODIES.each.len(), BODY_RULES.len());
    }

    // One header per rule, as each product sends it.
    #[test]
    fn headers_name_their_servers() {
        let samples: &[(&str, &str, &str, Option<&str>)] = &[
            ("CF-RAY", "8d2c1e9a7b3f1a2b-ICN", "cloudflare", None),
            ("cf-cache-status", "HIT", "cloudflare", None),
            ("X-Amz-Cf-Id", "a1b2c3", "amazon_cloudfront", None),
            ("X-Amz-Cf-Pop", "ICN57-P1", "amazon_cloudfront", None),
            ("X-Cache", "Miss from cloudfront", "amazon_cloudfront", None),
            (
                "Via",
                "1.1 abc123.cloudfront.net (CloudFront)",
                "amazon_cloudfront",
                None,
            ),
            ("X-Fastly-Request-ID", "a1b2", "fastly", None),
            ("X-Served-By", "cache-icn1234-ICN", "fastly", None),
            ("Akamai-GRN", "0.1a2b3c4d.1700000000.5e6f", "akamai", None),
            ("X-Akamai-Transformed", "9 - 0 pmb=mRUM,1", "akamai", None),
            ("X-Akamai-Request-ID", "1a2b3c", "akamai", None),
            ("X-Azure-Ref", "0abc", "azure_front_door", None),
            (
                "X-Iinfo",
                "1-2-3 NNNN CT(0 0 0) RT(1 2) q(0 0 0 0) r(1 1) U6",
                "imperva",
                None,
            ),
            ("X-CDN", "Imperva", "imperva", None),
            ("X-Sucuri-ID", "11005", "sucuri", None),
            ("X-Sucuri-Cache", "HIT", "sucuri", None),
            ("x-amz-apigw-id", "Abc=", "aws_api_gateway", None),
            (
                "x-amzn-ErrorType",
                "UnauthorizedException",
                "aws_api_gateway",
                None,
            ),
            ("x-amz-id-2", "abc+def=", "amazon_s3", None),
            (
                "x-goog-generation",
                "1700000000000000",
                "google_cloud_storage",
                None,
            ),
            ("X-GUploader-UploadID", "ABC", "google_cloud_storage", None),
            ("x-ms-blob-type", "BlockBlob", "azure_storage", None),
            ("X-Vercel-Id", "icn1::abc", "vercel", None),
            ("X-Vercel-Cache", "HIT", "vercel", None),
            ("X-NF-Request-ID", "01ABC", "netlify", None),
            ("Fly-Request-Id", "01ABC-nrt", "fly_io", None),
            ("X-GitHub-Request-Id", "ABCD:1234", "github", None),
            ("Via", "1.1 vegur", "heroku", None),
            ("Via", "1.1 google", "google_cloud", None),
            ("x-render-origin-server", "Render", "render", None),
            ("rndr-id", "abc-123", "render", None),
            ("x-envoy-upstream-service-time", "12", "envoy", None),
            ("X-Kong-Upstream-Latency", "3", "kong", None),
            ("Via", "1.1 kong/3.4.2", "kong", Some("3.4.2")),
            ("X-Varnish", "32770 3", "varnish", None),
            ("Via", "1.1 varnish (Varnish/7.4)", "varnish", None),
            ("Via", "NS-CACHE-10.0: 1", "citrix_adc", None),
            ("X-Runtime", "0.012345", "ruby", None),
            ("X-Debug-Token", "a1b2c3", "symfony", None),
            ("X-Debug-Token", "a1b2c3", "symfony_profiler", None),
            (
                "X-Debug-Token-Link",
                "https://example.com/_profiler/a1b2c3",
                "symfony_profiler",
                None,
            ),
            (
                "X-AspNet-Version",
                "4.0.30319",
                "asp_net",
                Some("4.0.30319"),
            ),
            ("X-AspNetMvc-Version", "5.2", "asp_net_mvc", Some("5.2")),
            ("X-Drupal-Cache", "HIT", "drupal", None),
            ("X-Drupal-Dynamic-Cache", "MISS", "drupal", None),
            ("X-LiteSpeed-Cache", "hit", "litespeed", None),
            ("X-Turbo-Charged-By", "LiteSpeed", "litespeed", None),
            (
                "X-Pingback",
                "https://example.com/xmlrpc.php",
                "wordpress",
                None,
            ),
            ("X-Shopify-Stage", "production", "shopify", None),
            ("X-ShopId", "123", "shopify", None),
            ("X-Wix-Request-Id", "1700000000.123", "wix", None),
            ("X-Magento-Tags", "store,cms_b", "magento", None),
            ("X-Magento-Cache-Debug", "HIT", "magento", None),
            ("X-ORACLE-DMS-ECID", "abc", "oracle_weblogic", None),
            ("X-Jenkins", "2.426.1", "jenkins", Some("2.426.1")),
            (
                "Server",
                "Payara Server 7.2026.10",
                "payara",
                Some("7.2026.10"),
            ),
            (
                "Server",
                "GlassFish Server Open Source Edition  4.1.2",
                "glassfish",
                Some("4.1.2"),
            ),
            (
                "X-Powered-By",
                "Servlet/6.0 JSP/3.1 (Payara Server 6.2024.6 Java/Azul Systems, Inc./17)",
                "payara",
                Some("6.2024.6"),
            ),
            ("X-Hudson", "1.395", "jenkins", None),
            (
                "X-Gitlab-Meta",
                "{\"correlation_id\":\"a\"}",
                "gitlab",
                None,
            ),
            ("X-AREQUESTID", "1234x5678x1", "atlassian", None),
            ("kbn-name", "kibana", "kibana", None),
            ("kbn-version", "8.11.1", "kibana", Some("8.11.1")),
            ("X-Elastic-Product", "Elasticsearch", "elasticsearch", None),
            (
                "X-JFrog-Version",
                "Artifactory/7.71.5 77105900",
                "jfrog_artifactory",
                Some("7.71.5"),
            ),
            ("X-Powered-By-Plesk", "PleskLin", "plesk", None),
            (
                "X-Backend-Server",
                "app-03.internal",
                "backend_host_header",
                None,
            ),
            ("X-Backend-Host", "10.0.0.5", "backend_host_header", None),
            ("X-Upstream", "10.0.0.5:8080", "backend_host_header", None),
            ("Server", "AmazonS3", "amazon_s3", None),
            ("Server", "awselb/2.0", "aws_elb", Some("2.0")),
            ("Server", "Google Frontend", "google_cloud", None),
            ("Server", "AkamaiGHost", "akamai", None),
            ("Server", "CloudFront", "amazon_cloudfront", None),
            ("Server", "Kestrel", "kestrel", None),
            (
                "Server",
                "Microsoft-HTTPAPI/2.0",
                "microsoft_httpapi",
                Some("2.0"),
            ),
            ("Server", "BigIP", "f5_big_ip", None),
            ("Server", "Netlify", "netlify", None),
            ("Server", "Vercel", "vercel", None),
            ("Server", "GitHub.com", "github", None),
            ("Server", "Tengine/2.3.3", "tengine", Some("2.3.3")),
            ("Server", "lighttpd/1.4.73", "lighttpd", Some("1.4.73")),
            (
                "Server",
                "Werkzeug/3.0.1 Python/3.12.0",
                "werkzeug",
                Some("3.0.1"),
            ),
            (
                "Server",
                "Werkzeug/3.0.1 Python/3.12.0",
                "python",
                Some("3.12.0"),
            ),
            ("Server", "TornadoServer/6.4", "tornado", Some("6.4")),
            ("Server", "Cowboy", "cowboy", None),
            (
                "Server",
                "nginx/1.24.0 + Phusion Passenger 6.0.18",
                "passenger",
                Some("6.0.18"),
            ),
            (
                "Server",
                "WEBrick/1.8.1 (Ruby/3.2.2/2023-03-30)",
                "webrick",
                Some("1.8.1"),
            ),
            (
                "Server",
                "WEBrick/1.8.1 (Ruby/3.2.2/2023-03-30)",
                "ruby",
                Some("3.2.2"),
            ),
            ("Server", "Undertow/1", "undertow", Some("1")),
            ("Server", "WildFly/10", "wildfly", Some("10")),
            ("Server", "JBoss-EAP/7", "jboss", None),
            (
                "Server",
                "Oracle-HTTP-Server-12c",
                "oracle_http_server",
                None,
            ),
            ("Server", "WebLogic Server 12.2.1", "oracle_weblogic", None),
            ("Server", "IBM_HTTP_Server", "ibm_http_server", None),
            (
                "Server",
                "WebSphere Application Server/9.0",
                "websphere",
                None,
            ),
            ("Server", "squid/5.7", "squid", Some("5.7")),
            ("Server", "Varnish", "varnish", None),
            ("Server", "istio-envoy", "istio", None),
            ("Server", "kong/3.4.2", "kong", Some("3.4.2")),
            ("Server", "Sucuri/Cloudproxy", "sucuri", None),
            (
                "Server",
                "Apache/2.4.58 (Unix) mod_security/2.9.7",
                "modsecurity",
                None,
            ),
            ("Server", "hypercorn-h11", "hypercorn", None),
            ("Server", "daphne", "daphne", None),
            ("Server", "CherryPy/18.9.0", "cherrypy", Some("18.9.0")),
            ("Server", "Deno/1.38.0", "deno", Some("1.38.0")),
            ("Server", "Apache/2.4.52 (Ubuntu)", "ubuntu", None),
            ("Server", "Apache/2.4.57 (Debian)", "debian", None),
            (
                "Server",
                "Apache/2.4.6 (CentOS) OpenSSL/1.0.2k-fips",
                "centos",
                None,
            ),
            (
                "Server",
                "Apache/2.4.37 (Red Hat Enterprise Linux)",
                "red_hat",
                None,
            ),
            (
                "Server",
                "Apache/2.4.58 (Win64) OpenSSL/3.1.3",
                "windows",
                None,
            ),
            ("Server", "Apache/2.4.58 (FreeBSD)", "freebsd", None),
            ("Server", "Apache/2.4.58 (Unix)", "unix", None),
            (
                "X-Powered-By",
                "Servlet/4.0 JSP/2.3",
                "java_servlet",
                Some("4.0"),
            ),
            ("X-Powered-By", "Servlet/4.0 JSP/2.3", "java", None),
            ("X-Powered-By", "Undertow/1", "undertow", Some("1")),
            ("X-Powered-By", "WildFly/26", "wildfly", Some("26")),
            ("X-Powered-By", "JBoss-7", "jboss", None),
            (
                "X-Powered-By",
                "Phusion Passenger 6.0.18",
                "passenger",
                Some("6.0.18"),
            ),
            ("X-Powered-By", "PleskLin", "plesk", None),
            ("X-Powered-By", "ARR/3.0", "iis_arr", Some("3.0")),
            ("X-Powered-By", "Sails <sailsjs.com>", "sails", None),
            ("X-Powered-By", "Mono", "dotnet", None),
        ];
        for (name, value, id, version) in samples {
            assert!(
                found(detect_headers([(*name, *value)])).contains(&one(id, *version)),
                "{id} {version:?} from {name}: {value}: {:?}",
                found(detect_headers([(*name, *value)]))
            );
        }
        // The frozen catalog's own products and plain values name nothing here.
        for (name, value) in [
            ("Server", "nginx/1.25.3"),
            ("X-Powered-By", "PHP/8.2.12"),
            ("Via", "1.1 proxy.example.com"),
            ("X-Cache", "HIT"),
            ("X-Served-By", "web-1"),
            ("X-Runtime", "fast"),
            ("Content-Type", "text/html"),
        ] {
            assert!(
                detect_headers([(name, value)]).is_empty(),
                "{name}: {value}"
            );
        }
    }

    // One Set-Cookie per rule; only its name decides.
    #[test]
    fn cookie_names_name_their_servers() {
        let samples: &[(&str, &str)] = &[
            ("JSESSIONID=ABC123; Path=/; HttpOnly", "java_servlet"),
            ("PHPSESSID=abc; path=/", "php"),
            (
                "ASP.NET_SessionId=abc; path=/; HttpOnly; SameSite=Lax",
                "asp_net",
            ),
            ("ASPSESSIONIDQASCRDTA=ABC; path=/", "asp_classic"),
            (".AspNetCore.Antiforgery.abc=CfDJ8; path=/", "asp_net_core"),
            ("laravel_session=eyJpdiI6; path=/; httponly", "laravel"),
            ("ci_session=abc; path=/", "codeigniter"),
            ("CAKEPHP=abc; path=/", "cakephp"),
            ("PLAY_SESSION=abc; Path=/", "play_framework"),
            ("connect.sid=s%3Aabc; Path=/; HttpOnly", "express"),
            ("csrftoken=abc; Path=/; SameSite=Lax", "django"),
            ("__VCAP_ID__=abc; Path=/", "cloud_foundry"),
            (
                "AWSALB=abc; Expires=Thu, 01 Jan 2026 00:00:00 GMT; Path=/",
                "aws_alb",
            ),
            ("AWSALBCORS=abc; Path=/; SameSite=None; Secure", "aws_alb"),
            ("AWSELB=abc; PATH=/", "aws_elb"),
            (
                "BIGipServerpool_web=1677787402.20480.0000; path=/",
                "f5_big_ip",
            ),
            ("TS01a2b3c4=01abcdef; Path=/", "f5_advanced_waf"),
            (
                "__cf_bm=abc; path=/; domain=.example.com; HttpOnly; Secure",
                "cloudflare",
            ),
            ("incap_ses_123_456=abc; path=/", "imperva"),
            ("visid_incap_456=abc; path=/", "imperva"),
            ("ak_bmsc=abc; Domain=.example.com; Path=/", "akamai"),
            ("NSC_vsv_xfc=ffffffff09; path=/", "citrix_adc"),
            ("BNI__BARRACUDA_LB_COOKIE=abc; path=/", "barracuda"),
            ("FORTIWAFSID=abc; path=/", "fortiweb"),
            (
                "KEYCLOAK_SESSION=realm/abc; Path=/realms/realm/",
                "keycloak",
            ),
            ("wordpress_logged_in_0123abcd=admin%7C; path=/", "wordpress"),
            ("SESS0123456789abcdef0123456789abcdef=abc; path=/", "drupal"),
            ("mage-cache-sessid=true; path=/", "magento"),
            ("_shopify_y=abc; path=/", "shopify"),
        ];
        for (cookie, id) in samples {
            assert!(
                found(detect_headers([("Set-Cookie", *cookie)])).contains(&one(id, None)),
                "{id} from {cookie}"
            );
        }
        assert!(detect_headers([("Set-Cookie", "session=abc; Path=/")]).is_empty());
        // A cookie value never names anything.
        assert!(detect_headers([("Set-Cookie", "theme=JSESSIONID; Path=/")]).is_empty());
    }

    #[test]
    fn a_bigip_cookie_that_encodes_an_address_is_an_exposure() {
        for value in [
            "BIGipServerpool_web=1677787402.20480.0000; path=/",
            "BIGipServerpool_rd=rd5o00000000000000000000ffffc0a80164o80; path=/",
        ] {
            assert!(
                found(detect_headers([("Set-Cookie", value)]))
                    .contains(&one("backend_address", None)),
                "{value}"
            );
        }
        // An encrypted persistence cookie names the product only.
        let encrypted = found(detect_headers([(
            "Set-Cookie",
            "BIGipServerpool_web=!Zb4R7m2k1/abcdEFGH; path=/",
        )]));
        assert_eq!(encrypted, [one("f5_big_ip", None)]);
    }

    // One body per rule, as each server writes it.
    #[test]
    fn bodies_name_their_servers_and_exposures() {
        let samples: &[(&str, &str, Option<&str>)] = &[
            ("<hr><center>nginx/1.25.3</center>\r\n</body>", "nginx", Some("1.25.3")),
            ("<hr><center>nginx</center>", "nginx", None),
            ("<hr><center>openresty/1.21.4.1</center>", "openresty", Some("1.21.4.1")),
            ("<p>Powered by Tengine/2.3.3</p>", "tengine", Some("2.3.3")),
            ("<address>Apache/2.4.58 (Ubuntu) Server at example.com Port 80</address>", "apache_http_server", Some("2.4.58")),
            ("<address>Apache/2.4.58 (Ubuntu) Server at example.com Port 80</address>", "ubuntu", None),
            ("<address>Apache/2.4.57 (Debian) Server at example.com Port 443</address>", "debian", None),
            ("<address>Apache/2.4.6 (CentOS) OpenSSL/1.0.2k-fips Server at example.com Port 80</address>", "centos", None),
            ("<address>Apache/2.4.37 (Red Hat Enterprise Linux) Server at example.com Port 80</address>", "red_hat", None),
            ("<address>Apache/2.4.58 (Win64) OpenSSL/3.1.3 Server at example.com Port 80</address>", "windows", None),
            ("<address>Apache/2.4.58 (Unix) Server at example.com Port 80</address>", "unix", None),
            ("<title>Index of /uploads</title>", "directory_listing", None),
            ("<title>IIS 10.0 Detailed Error - 404.0 - Not Found</title>", "microsoft_iis", Some("10.0")),
            ("<title>IIS 10.0 Detailed Error - 500.19</title>", "iis_error", None),
            ("<title>IIS Windows Server</title>", "microsoft_iis", None),
            ("<title>Welcome to WildFly</title>", "wildfly", None),
            ("<html><head><title>Error</title></head><body>404 - Not Found</body></html>", "undertow", None),
            ("<html><head><title>Error</title></head><body>Not Found</body></html>", "undertow", None),
            ("<title>Open Liberty</title>", "open_liberty", None),
            ("<!-- Licensed Materials - Property of the Open Liberty project -->", "open_liberty", None),
            ("Error 404: java.io.FileNotFoundException: SRVE0190E: File not found: /x", "websphere", None),
            ("<title>Payara Server 7.2026.10- Error report</title>", "payara", Some("7.2026.10")),
            ("<title>GlassFish Server Open Source Edition  4.1.2  - Error report</title>", "glassfish", Some("4.1.2")),
            ("<script>window.grafanaBootData = {", "grafana", None),
            (r#""buildInfo":{"hideVersion":false,"version":"13.2.4","commit":"x"}"#, "grafana", Some("13.2.4")),
            ("<hr class=\"line\" /><h3>Apache Tomcat/9.0.83</h3></body>", "apache_tomcat", Some("9.0.83")),
            ("<hr/><a href=\"https://jetty.org/\">Powered by Jetty:// 10.0.18</a><hr/>", "jetty", Some("10.0.18")),
            ("JBWEB000065: HTTP Status 404", "jboss", None),
            ("<html><body><h1>Whitelabel Error Page</h1><p>This application has no explicit mapping", "spring_boot", None),
            (r#"{"timestamp":"2026-10-16T09:00:00.000+00:00","status":404,"error":"Not Found","path":"/x"}"#, "spring_boot", None),
            (r#"{"_links":{"self":{"href":"https://example.com/actuator","templated":false}}}"#, "spring_boot", None),
            (r#"{"_links":{"self":{"href":"https://example.com/actuator","templated":false}}}"#, "spring_actuator", None),
            ("<pre>Cannot GET /missing</pre>", "express", None),
            ("You're seeing this error because you have <code>DEBUG = True</code> in your Django settings file.", "django", None),
            ("You&#x27;re seeing this error because you have <code>DEBUG = True</code>", "django_debug", None),
            ("<p>CSRF verification failed. Request aborted.</p>", "django", None),
            (r#"{"detail":"Authentication credentials were not provided."}"#, "django_rest_framework", None),
            (r#"{"detail":"Not Found"}"#, "fastapi", None),
            (r#"{"detail":[{"type":"missing","loc":["body","name"],"msg":"Field required"}]}"#, "fastapi", None),
            ("<p>The requested URL was not found on the server. If you entered the URL manually please check your spelling and try again.</p>", "flask", None),
            ("The debugger caught an exception in your WSGI application.", "werkzeug_debugger", None),
            ("<script>window.ignite = window.ignite || {};</script>", "laravel", None),
            (r#""exception":"Illuminate\\Database\\QueryException""#, "laravel_debug", None),
            ("<title>Symfony Exception</title>", "symfony", None),
            ("<div class=\"sf-toolbar\">", "symfony_debug", None),
            ("<title>Action Controller: Exception caught</title>", "ruby_on_rails", None),
            ("<title>Action Controller: Exception caught</title>", "rails_debug", None),
            ("<h1>The page you were looking for doesn't exist.</h1>", "ruby_on_rails", None),
            ("<h1>Server Error in '/' Application.</h1>", "asp_net", None),
            ("<h1>Server Error in '/shop' Application.</h1>", "aspnet_error", None),
            ("<b>Version Information:</b>&nbsp;Microsoft .NET Framework Version:4.0.30319; ASP.NET Version:4.8.4465.0", "asp_net", Some("4.8.4465.0")),
            ("<b>Version Information:</b>&nbsp;Microsoft .NET Framework Version:4.0.30319; ASP.NET Version:4.8.4465.0", "dotnet", Some("4.0.30319")),
            ("<br />\n<b>Warning</b>:  Undefined variable $id in <b>/var/www/html/item.php</b> on line <b>12</b><br />", "php", None),
            ("<b>Fatal error</b>:  Uncaught Error in /srv/app.php on line <b>3</b>", "php_error", None),
            ("<title>PHP 8.2.12 - phpinfo()</title>", "php", Some("8.2.12")),
            ("<title>PHP 8.2.12 - phpinfo()</title>", "phpinfo", None),
            ("\tat org.springframework.web.servlet.DispatcherServlet.doDispatch(DispatcherServlet.java:1089)", "java", None),
            ("\tat java.lang.Thread.run(Thread.java:840)", "java_stack_trace", None),
            ("   at System.Web.Mvc.Controller.Execute(RequestContext ctx) in C:\\src\\Home.cs:line 42", "dotnet", None),
            ("--- End of stack trace from previous location ---", "dotnet_stack_trace", None),
            ("Traceback (most recent call last):\n  File \"app.py\", line 3", "python", None),
            ("Traceback (most recent call last):", "python_traceback", None),
            ("    at Object.<anonymous> (/app/index.js:3:9)", "node_js", None),
            ("    at Module._compile (node:internal/modules/cjs/loader:1256:14)", "node_stack_trace", None),
            ("goroutine 1 [running]:\nmain.main()", "go", None),
            ("goroutine 7 [running]:", "go_panic", None),
            ("404 page not found\n", "go", None),
            ("<html><body><h1>503 Service Unavailable</h1>\nNo server is available to handle this request.\n</body></html>", "haproxy", None),
            ("upstream connect error or disconnect/reset before headers. reset reason: connection failure", "envoy", None),
            ("no healthy upstream", "envoy", None),
            ("<p>Guru Meditation:</p><p>XID: 32770</p>", "varnish", None),
            ("Generated Thu, 16 Oct 2026 09:00:00 GMT by proxy (squid/5.7)", "squid", Some("5.7")),
            ("<div id=\"cf-error-details\" class=\"cf-error-details-wrapper\">", "cloudflare", None),
            ("Reference&#32;&#35;18&#46;2f3b1c17&#46;1700000000&#46;5e6f7a", "akamai", None),
            ("Reference&#32;&#35;18.2f3b1c17.1700000000.5e6f7a", "akamai", None),
            ("Generated by cloudfront (CloudFront)", "amazon_cloudfront", None),
            ("Request unsuccessful. Incapsula incident ID: 123-456", "imperva", None),
            ("<title>Sucuri WebSite Firewall - Access Denied</title>", "sucuri", None),
            ("This error was generated by Mod_Security.", "modsecurity", None),
            (r#"{"message":"no Route matched with those values"}"#, "kong", None),
            (r#"{"message":"Missing Authentication Token"}"#, "aws_api_gateway", None),
            ("<html><body>default backend - 404</body></html>", "ingress_nginx", None),
            ("{\n  \"version\" : {\n    \"number\" : \"7.17.15\",\n    \"build_flavor\" : \"default\"\n  },\n  \"tagline\" : \"You Know, for Search\"\n}", "elasticsearch", Some("7.17.15")),
            ("\"tagline\" : \"You Know, for Search\"", "elasticsearch_root", None),
            (r#"{"errors":[{"message":"Cannot query field","extensions":{"code":"GRAPHQL_VALIDATION_FAILED"}}]}"#, "apollo_server", None),
            ("<title>Swagger UI</title>", "api_docs", None),
            (r#"{"openapi":"3.0.1","info":{"title":"API"}}"#, "api_docs", None),
            ("You have an error in your SQL syntax; check the manual that corresponds to your MySQL server version", "mysql", None),
            ("org.postgresql.util.PSQLException: ERROR: syntax error at or near \"'\"", "postgresql", None),
            ("ORA-00933: SQL command not properly ended", "oracle_database", None),
            ("Unclosed quotation mark after the character string ''.", "sql_server", None),
            ("sqlite3.OperationalError: near \"'\": syntax error", "sqlite", None),
            ("MongoServerError: E11000 duplicate key error collection", "mongodb", None),
            ("WRONGTYPE Operation against a key holding the wrong kind of value", "redis", None),
            ("ORA-01756: quoted string not properly terminated", "sql_error", None),
        ];
        for (body, id, version) in samples {
            assert!(
                found(detect_body(body)).contains(&one(id, *version)),
                "{id} {version:?} in {body}: {:?}",
                found(detect_body(body))
            );
        }
        for body in [
            "<p>An ordinary page about nginx and Apache.</p>",
            r#"{"detail":"Order shipped"}"#,
            r#"{"items":[],"status":200}"#,
            "404 page not found on this site, sorry",
        ] {
            assert!(
                detect_body(body).is_empty(),
                "{body}: {:?}",
                found(detect_body(body))
            );
        }
    }

    // One sample per exposure rule; the finding is reported, not the value.
    // Each key is split with concat! so secret scanning does not take a test
    // sample for a leaked key and refuse the push.
    #[test]
    fn bodies_give_away_source_maps_credentials_and_internal_addresses() {
        let samples: &[(&str, &str)] = &[
            (
                "console.log(1);\n//# sourceMappingURL=main.3f2a.js.map",
                "source_map",
            ),
            (
                "//# sourceMappingURL=data:application/json;base64,eyJ2",
                "source_map",
            ),
            (
                concat!("var k=\"AKIA", "IOSFODNN7EXAMPLE\";"),
                "aws_access_key",
            ),
            (
                concat!("apiKey:\"AIza", "SyA1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q\","),
                "google_api_key",
            ),
            (
                concat!("token=\"xox", "b-1234567890-abcdefghij\""),
                "slack_token",
            ),
            (
                concat!(
                    "https://hooks.slack.com/services/",
                    "T0123ABC/B0123ABC/a1b2c3d4e5f6"
                ),
                "slack_webhook",
            ),
            (
                concat!("\"ghp", "_0123456789abcdefghijklmnopqrstuvwxyz\""),
                "github_token",
            ),
            (
                concat!("stripe(\"sk", "_live_0123456789abcdefghijklmn\")"),
                "stripe_secret_key",
            ),
            (
                concat!(
                    "SG",
                    ".0123456789abcdefghijkl.0123456789abcdefghijklmnopqrstuvwxyzABCDEFG"
                ),
                "sendgrid_key",
            ),
            (
                concat!(
                    "-----BEGIN RSA PRIVATE",
                    " KEY-----\nMIIEowIBAAKCAQEAu1SU1LfVLPHCozMxH2Mo4lgOEePzNm0tRgeLezV6ffAt0gun"
                ),
                "private_key",
            ),
            (
                concat!(
                    "var k=\"-----BEGIN PRIVATE",
                    " KEY-----\\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7\";"
                ),
                "private_key",
            ),
            (
                concat!(
                    "Bearer eyJhbGciOiJIUzI1NiJ9.",
                    "eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U"
                ),
                "jwt",
            ),
        ];
        for (body, id) in samples {
            for kind in [Exposed::Script, Exposed::Page, Exposed::Error] {
                let expected = !(*id == "jwt" && matches!(kind, Exposed::Page));
                assert_eq!(
                    found(detect_exposures(body, kind)).contains(&one(id, None)),
                    expected,
                    "{id} in {body}"
                );
            }
        }
        let address = |body: &str, kind| {
            found(detect_exposures(body, kind)).contains(&one("private_ip", None))
        };
        assert!(address(
            r#"fetch("http://10.1.2.3:8080/api/items")"#,
            Exposed::Script
        ));
        assert!(address(
            "connect ECONNREFUSED 192.168.0.12:5432",
            Exposed::Error
        ));
        assert!(
            !address("<p>Your router is at 192.168.0.1.</p>", Exposed::Page),
            "a page's prose"
        );
        assert!(
            !address("var version=\"10.0.0.1\";", Exposed::Script),
            "a script's number, not its URL"
        );
        assert!(
            !address("build 1.10.0.12 and v192.168.1.1", Exposed::Error),
            "a longer dotted number"
        );
        for body in [
            "var k=\"AKIA\";",
            "AIzaSy-too-short",
            concat!("sk", "_test_0123456789abcdefghijklmn"),
            "-----BEGIN PUBLIC KEY-----",
            // A PEM parser's own constant, with no key after it.
            concat!("var h=\"-----BEGIN PRIVATE", " KEY-----\";"),
            "//# sourceURL=app.js",
        ] {
            assert!(detect_exposures(body, Exposed::Script).is_empty(), "{body}");
        }
        assert_eq!(
            found(detect_headers([("SourceMap", "/static/app.js.map")])),
            [one("source_map", None)]
        );
    }

    #[test]
    fn server_signals_stay_with_the_origin_that_responded() {
        let header = detect_headers([("CF-RAY", "1")]).remove(0);
        let body = detect_body("<hr><center>nginx</center>").remove(0);
        let cookie = detect_headers([("Set-Cookie", "PHPSESSID=a")]).remove(0);
        assert!([header, body, cookie].iter().all(server_signal));
        let page = crate::service_map_libraries::detect(
            "<div data-v-1a2b3c4d>",
            crate::service_map_libraries::Kind::Page,
        );
        assert!(!server_signal(&page[0]));
    }
}
