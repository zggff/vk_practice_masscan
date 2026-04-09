use std::time::{Duration, SystemTime, UNIX_EPOCH};

use log::debug;
use reqwest::Client;

const VK_API_VERSION: &str = "5.199";

pub async fn send_vk(
    access_token: &str,
    user_id: &str,
    message: &str,
) -> Result<(), reqwest::Error> {
    let client = Client::new();
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let res = client
        .post("https://api.vk.com/method/messages.send")
        .timeout(Duration::from_secs(5))
        .form(&[
            ("user_id", user_id),
            ("message", message),
            ("random_id", &format!("{}", time)),
            ("access_token", access_token),
            ("v", VK_API_VERSION),
        ])
        .send()
        .await?;
    log::debug!("{}", res.text().await?);
    Ok(())
}

pub async fn send_telegram(
    bot_token: &str,
    chat_id: &str,
    msg: &str,
) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::new();
    let res = client
        .post(format!(
            "https://api.telegram.org/bot{}/sendMessage",
            bot_token
        ))
        .timeout(Duration::from_secs(5))
        .form(&[("chat_id", chat_id), ("text", msg)])
        .send()
        .await?;
    debug!("{}", res.text().await?);
    Ok(())
}
