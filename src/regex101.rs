use reqwest;
use serde::Deserialize;
use serde_json::json;

use crate::error::Error;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(crate) struct Regex101Session {
    pub version_delete_code: String,
    pub regex_delete_code: String,
    pub permalink_fragment: String,
    pub version: i32,
    pub is_library_entry: bool,
}

pub(crate) fn upload(pattern: String) -> Result<Regex101Session, Error> {
    // `substitution`, `listSubstitution`, and `unitTests` are all rejected as
    // missing if we leave them out, even though we have nothing to put in
    // them.
    let body = json!({
        "regex": pattern,
        "flags": "gm",
        "testString": "Enter your test content here.",
        "flavor": "pcre2",
        "delimiter": "/",
        "substitution": "",
        "listSubstitution": "",
        "unitTests": [],
    });

    let resp = reqwest::blocking::Client::new()
        .post("https://regex101.com/api/regex")
        .json(&body)
        .send()?;

    let body = resp.text()?;
    let session: Regex101Session = serde_json::from_str(&body)?;

    Ok(session)
}
