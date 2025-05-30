//! App URL builder

use std::fmt::Display;

static BASE_URL: &str = "https://app.foxglove.dev";

#[derive(Debug, Clone)]
enum DataSource {
    WebSocket(String),
}

/// A foxglove app URL.
///
/// This struct implements [`Display`] by formatting the URL, so you can use it directly in format
/// strings, or use `.to_string()` to convert it to a string.
///
/// # Example
///
/// ```
/// use foxglove::AppUrl;
///
/// let url = AppUrl::new()
///     .with_layout_id("lay_1234")
///     .with_websocket("ws://localhost:8765");
/// println!("Click here: {url}");
/// assert_eq!(format!("{url}"), url.to_string());
/// ```
#[must_use]
#[derive(Debug, Default, Clone)]
pub struct AppUrl {
    data_source: Option<DataSource>,
    layout_id: Option<String>,
    open_in_desktop: bool,
}

impl Display for AppUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(BASE_URL)?;
        for (ii, (k, v)) in self.query_params().into_iter().enumerate() {
            let sep = if ii == 0 { '?' } else { '&' };
            let venc = urlencoding::encode(v);
            write!(f, "{sep}{k}={venc}")?;
        }
        Ok(())
    }
}

impl AppUrl {
    /// Creates a new app URL.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a vector of URL query parameters.
    ///
    /// The parameter values are not URL-encoded.
    fn query_params(&self) -> Vec<(&str, &str)> {
        let mut params = vec![];
        if let Some(ds) = &self.data_source {
            match ds {
                DataSource::WebSocket(url) => {
                    params.extend([("ds", "foxglove-websocket"), ("ds.url", url)])
                }
            }
        }
        if let Some(layout_id) = &self.layout_id {
            params.push(("layoutId", layout_id));
        }
        if self.open_in_desktop {
            params.push(("openIn", "desktop"));
        }
        params
    }

    /// Sets the layout ID.
    ///
    /// If no layout ID is specified, the app will use the most recently-used layout.
    pub fn with_layout_id(mut self, layout_id: impl Into<String>) -> Self {
        self.layout_id = Some(layout_id.into());
        self
    }

    /// Constructs a desktop URL, rather than a web URL.
    pub fn with_open_in_desktop(mut self) -> Self {
        self.open_in_desktop = true;
        self
    }

    /// Sets a websocket data source.
    pub fn with_websocket(mut self, url: impl Into<String>) -> Self {
        self.data_source = Some(DataSource::WebSocket(url.into()));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_url() {
        assert_eq!(AppUrl::new().to_string(), BASE_URL);
        assert_eq!(
            AppUrl::new().with_layout_id("lay_123").to_string(),
            format!("{BASE_URL}?layoutId=lay_123")
        );
        assert_eq!(
            AppUrl::new().with_open_in_desktop().to_string(),
            format!("{BASE_URL}?openIn=desktop")
        );
        assert_eq!(
            AppUrl::new()
                .with_websocket("ws://1.2.3.4:1234")
                .to_string(),
            format!("{BASE_URL}?ds=foxglove-websocket&ds.url=ws%3A%2F%2F1.2.3.4%3A1234")
        );
        assert_eq!(
            AppUrl::new()
                .with_layout_id("lay_123")
                .with_open_in_desktop()
                .with_websocket("wss://my.robot.dev:9999")
                .to_string(),
            format!(
                "{BASE_URL}?ds=foxglove-websocket&ds.url=wss%3A%2F%2Fmy.robot.dev%3A9999\
                &layoutId=lay_123&openIn=desktop"
            )
        );
    }
}
