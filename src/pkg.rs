use reqwest;
use serde::Deserialize;

use crate::error::Error;

const PKGS: &str = "https://raw.githubusercontent.com/vale-cli/packages/master/library.json";

#[derive(Deserialize, Debug, Clone)]
pub struct Package {
    pub name: String,
    pub description: String,
    pub homepage: String,
}

pub async fn fetch() -> Result<Vec<Package>, Error> {
    let resp = reqwest::get(PKGS).await?;
    let info: Vec<Package> = resp.json().await?;
    Ok(info)
}
