use anyhow::{bail, ensure, Context};
use url::{Host, Url};

/// The advertised URL and actual listener can differ behind a reverse proxy.
#[derive(Debug)]
pub struct ServerConfig {
    pub public_base_url: String,
    pub bind_addr: String,
}

impl ServerConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::parse(
            &std::env::var("HOST_URL").unwrap_or_else(|_| "http://localhost:3000".into()),
            std::env::var("BIND_ADDR").ok().as_deref(),
            std::env::var("PUBLIC_BASE_URL").ok().as_deref(),
        )
    }

    fn parse(
        host_url: &str,
        bind_addr: Option<&str>,
        public_url: Option<&str>,
    ) -> anyhow::Result<Self> {
        let host = origin(host_url, "HOST_URL")?;
        let bind_addr = match bind_addr {
            Some(value) => {
                ensure!(!value.trim().is_empty(), "BIND_ADDR must not be empty");
                value.to_owned()
            }
            None => {
                ensure!(host.scheme() == "http", "HTTPS requires a TLS reverse proxy: set BIND_ADDR to the internal HTTP listener, e.g. 127.0.0.1:3000");
                let hostname = match host.host().context("HOST_URL must contain a host")? {
                    Host::Ipv6(ip) => format!("[{ip}]"),
                    other => other.to_string(),
                };
                format!("{hostname}:{}", host.port_or_known_default().unwrap())
            }
        };
        let public = match public_url {
            Some(value) => origin(value, "PUBLIC_BASE_URL")?,
            None => host,
        };
        Ok(Self {
            public_base_url: public.as_str().trim_end_matches('/').to_owned(),
            bind_addr,
        })
    }
}

fn origin(value: &str, name: &str) -> anyhow::Result<Url> {
    let url = Url::parse(value).with_context(|| format!("{name} must be a valid HTTP(S) URL"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
        bail!("{name} must be an HTTP(S) URL with a hostname");
    }
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "{name} must not contain credentials"
    );
    ensure!(
        url.path() == "/" && url.query().is_none() && url.fragment().is_none(),
        "{name} must be an origin without a path, query, or fragment"
    );
    ensure!(url.port() != Some(0), "{name} must use a nonzero port");
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_listener_and_public_origin() {
        for (url, bind, public) in [
            (
                "http://localhost:3000/",
                "localhost:3000",
                "http://localhost:3000",
            ),
            (
                "http://127.0.0.1:4567",
                "127.0.0.1:4567",
                "http://127.0.0.1:4567",
            ),
            (
                "http://cargo.example.com:8080/",
                "cargo.example.com:8080",
                "http://cargo.example.com:8080",
            ),
            ("http://[::1]:4567/", "[::1]:4567", "http://[::1]:4567"),
            ("http://localhost/", "localhost:80", "http://localhost"),
        ] {
            let config = ServerConfig::parse(url, None, None).unwrap();
            assert_eq!(config.bind_addr, bind);
            assert_eq!(config.public_base_url, public);
        }
    }

    #[test]
    fn supports_proxy_and_legacy_overrides() {
        let config =
            ServerConfig::parse("https://cargo.example.com/", Some("127.0.0.1:4000"), None)
                .unwrap();
        assert_eq!(config.bind_addr, "127.0.0.1:4000");
        assert_eq!(config.public_base_url, "https://cargo.example.com");
        let config = ServerConfig::parse(
            "http://localhost:3000",
            Some("0.0.0.0:8080"),
            Some("https://files.example.com/"),
        )
        .unwrap();
        assert_eq!(config.bind_addr, "0.0.0.0:8080");
        assert_eq!(config.public_base_url, "https://files.example.com");
    }

    #[test]
    fn rejects_invalid_or_misleading_origins() {
        for value in [
            "localhost:3000",
            "ftp://example.com",
            "http://localhost:0",
            "http://user:pass@localhost:3000",
            "http://localhost:3000/path",
            "http://localhost:3000/?query=1",
            "http://localhost:3000/#fragment",
        ] {
            assert!(ServerConfig::parse(value, None, None).is_err(), "{value}");
        }
        assert!(ServerConfig::parse("https://localhost:3000", None, None).is_err());
        assert!(ServerConfig::parse("http://localhost:3000", Some(""), None).is_err());
        assert!(ServerConfig::parse("http://localhost:3000", None, Some("not-a-url")).is_err());
    }
}
