use reqwest::{Client, Response};
use webpage::HTML;

use crate::error::Error;

async fn fetch(url: &str) -> Result<Response, Error> {
    let client = Client::builder().user_agent("Twitterbot/1.0").build()?;

    Ok(client.get(url).send().await?)
}

pub async fn link_to_img(url: &str) -> Result<Option<Vec<u8>>, Error> {
    let body = fetch(url).await?.text().await?;
    let html = HTML::from_string(body, None)?;

    let image = if let Some(x) = html.opengraph.images.first() {
        Some(fetch(&x.url).await?.bytes().await?.to_vec())
    } else {
        None
    };

    Ok(image)
}
