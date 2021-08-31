use serenity::async_trait;
use serenity::client::{Client, Context, EventHandler};
use serenity::framework::standard::StandardFramework;
use serenity::model::{gateway::Ready, id::ChannelId};
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

async fn previous_garfield(days_ago: i64) -> Option<String> {
    let utc = chrono::offset::Utc::now() - chrono::Duration::days(days_ago);
    let date_string = utc.format("%Y/%m/%d");
    let url = format!("https://www.gocomics.com/garfield/{}", date_string);

    // .ok() transforms the Result<T, E> into an Option<T>
    let res = reqwest::get(url).await.ok()?;
    let html: &str = &res.text().await.ok()?;

    select::document::Document::from(html)
        .find(select::predicate::Class("item-comic-image"))
        .next()
        .and_then(|node| node.first_child())
        .and_then(|node| node.attr("src"))
        .map(|url| url.to_owned())
}

async fn todays_garfield() -> Option<String> {
    // If today's comic can't be found, try yesterday's
    match previous_garfield(0).await {
        Some(url) => Some(url),
        None => previous_garfield(1).await,
    }
}

fn duration_until_time(hour: u32, minute: u32) -> Option<Duration> {
    if hour > 23 || minute > 59 {
        return None;
    }

    // Add the time onto today's date
    let time_today = chrono::offset::Local::today().and_hms(hour, minute, 0);

    // Add a day if the time has already passed
    let now = chrono::offset::Local::now();
    let date_time = match now >= time_today {
        true => time_today + chrono::Duration::days(1),
        false => time_today,
    };

    let std_duration = (date_time - now).to_std().ok()?;
    Some(std_duration)
}

struct Handler {
    channel_ids: Vec<ChannelId>,
    start_delay: Duration,
}
#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("Connected as: {}", ready.user.name);

        // Wrap context inside of an Arc<T> so it can be shared across threads
        let ctx = Arc::new(ctx);
        let delay_copy = self.start_delay;

        // Iterate over copies of the channel IDs
        for channel_id in self.channel_ids.iter().cloned() {
            let ctx_clone = Arc::clone(&ctx);
            // Start the message loop
            tokio::spawn(async move {
                tokio::time::sleep(delay_copy).await;
                loop {
                    let message = todays_garfield().await.unwrap_or_else(|| {
                        "I couldn't find the Garfield comic :cold_sweat:".to_owned()
                    });

                    if channel_id.say(&ctx_clone, message).await.is_err() {
                        let channel_name = channel_id
                            .name(&ctx_clone)
                            .await
                            .unwrap_or_else(|| "unknown channel".to_owned());

                        println!("Failed to send message in: {}", channel_name);
                    }
                    tokio::time::sleep(Duration::from_secs(60 * 60 * 24)).await;
                }
            });
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut settings = config::Config::default();
    settings.merge(config::File::with_name("config"))?;

    let channel_ids: Vec<ChannelId> = settings.get("channelIds")?;
    let token: String = settings.get("token")?;
    // 7:11PM is Garfield time
    let delay = duration_until_time(19, 11).unwrap_or_else(Duration::default);

    let handler = Handler {
        channel_ids: channel_ids,
        start_delay: delay,
    };
    let mut client = Client::builder(token)
        .event_handler(handler)
        .framework(StandardFramework::new())
        .await?;

    client.start().await?;
    Ok(())
}
