use crate::prelude::{Security, get_reqwest_blocking_client, get_reqwest_client};

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Release {
    pub version: semver::Version,
    pub url: String,
}

impl Release {
    const URL: &'static str = "https://api.github.com/repos/mtkennerly/ludusavi/releases/latest";

    pub async fn fetch(security: Security) -> Result<Self, crate::prelude::AnyError> {
        #[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
        pub struct Response {
            pub html_url: String,
            pub tag_name: String,
        }

        let req = get_reqwest_client(security)
            .get(Self::URL)
            .header(reqwest::header::USER_AGENT, &*crate::prelude::USER_AGENT);
        let res = req.send().await?;

        match res.status() {
            reqwest::StatusCode::OK => {
                let bytes = res.bytes().await?.to_vec();
                let raw = String::from_utf8(bytes)?;
                let parsed = serde_json::from_str::<Response>(&raw)?;

                Ok(Self {
                    version: semver::Version::parse(parsed.tag_name.trim_start_matches('v'))?,
                    url: parsed.html_url,
                })
            }
            code => Err(format!("status code: {code:?}").into()),
        }
    }

    pub fn fetch_sync(security: Security) -> Result<Self, crate::prelude::AnyError> {
        #[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
        pub struct Response {
            pub html_url: String,
            pub tag_name: String,
        }

        let req = get_reqwest_blocking_client(security)
            .get(Self::URL)
            .header(reqwest::header::USER_AGENT, &*crate::prelude::USER_AGENT);
        let res = req.send()?;

        match res.status() {
            reqwest::StatusCode::OK => {
                let bytes = res.bytes()?.to_vec();
                let raw = String::from_utf8(bytes)?;
                let parsed = serde_json::from_str::<Response>(&raw)?;

                Ok(Self {
                    version: semver::Version::parse(parsed.tag_name.trim_start_matches('v'))?,
                    url: parsed.html_url,
                })
            }
            code => Err(format!("status code: {code:?}").into()),
        }
    }

    pub fn is_update(&self) -> bool {
        if let Ok(current) = semver::Version::parse(*crate::prelude::VERSION) {
            self.version > current
        } else {
            false
        }
    }
}

/// Repository whose releases this fork installs. The auto-updater deliberately
/// never installs upstream releases, so fork features (like the scheduler) are
/// preserved across updates.
pub const FORK_REPO: &str = "Woesch-Nich/ludusavi";

/// A release of the fork, with the assets needed for verified auto-install.
#[derive(Debug, Clone)]
pub struct ForkRelease {
    pub tag: String,
    pub url: String,
    pub zip_url: String,
    pub sha256_url: Option<String>,
}

impl ForkRelease {
    pub async fn fetch(security: Security) -> Result<Self, crate::prelude::AnyError> {
        #[derive(Debug, serde::Deserialize)]
        struct Asset {
            name: String,
            browser_download_url: String,
        }

        #[derive(Debug, serde::Deserialize)]
        struct Response {
            html_url: String,
            tag_name: String,
            assets: Vec<Asset>,
        }

        let url = format!("https://api.github.com/repos/{FORK_REPO}/releases/latest");
        let req = get_reqwest_client(security)
            .get(&url)
            .header(reqwest::header::USER_AGENT, &*crate::prelude::USER_AGENT);
        let res = req.send().await?;

        match res.status() {
            reqwest::StatusCode::OK => {
                let bytes = res.bytes().await?.to_vec();
                let raw = String::from_utf8(bytes)?;
                let parsed: Response = serde_json::from_str(&raw)?;

                let zip = parsed
                    .assets
                    .iter()
                    .find(|a| a.name.ends_with("-windows-x64.zip"))
                    .ok_or("latest fork release has no Windows x64 zip asset")?;
                let sha256_url = parsed
                    .assets
                    .iter()
                    .find(|a| a.name == format!("{}.sha256", zip.name))
                    .map(|a| a.browser_download_url.clone());

                Ok(Self {
                    tag: parsed.tag_name,
                    url: parsed.html_url,
                    zip_url: zip.browser_download_url.clone(),
                    sha256_url,
                })
            }
            code => Err(format!("status code: {code:?}").into()),
        }
    }

    /// True when this release differs from the release tag baked into the
    /// running build. Development builds without a baked tag never auto-update.
    pub fn is_update(&self) -> bool {
        match crate::prelude::RELEASE_TAG {
            Some(baked) => self.tag != baked,
            None => false,
        }
    }

    pub fn semver(&self) -> semver::Version {
        semver::Version::parse(self.tag.trim_start_matches('v')).unwrap_or(semver::Version::new(0, 0, 0))
    }
}
