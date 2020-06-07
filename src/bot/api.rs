use crate::bot::logic;
use crate::bot::logic::{DeleteSubscriptionError, SubscriptionError};
use crate::db;
use crate::db::telegram::NewTelegramChat;
use futures::StreamExt;
use std::env;
use telegram_bot::prelude::*;
use telegram_bot::{Api, Error, Message, MessageChat, MessageKind, UpdateKind, UserId};

static SUBSCRIBE: &str = "/subscribe";
static LIST_SUBSCRIPTIONS: &str = "/list_subscriptions";
static SET_TIMEZONE: &str = "/set_timezone";
static GET_TIMEZONE: &str = "/get_timezone";
static UNSUBSCRIBE: &str = "/unsubscribe";
static HELP: &str = "/help";
static START: &str = "/start";

impl From<MessageChat> for NewTelegramChat {
    fn from(message_chat: MessageChat) -> Self {
        match message_chat {
            MessageChat::Private(chat) => NewTelegramChat {
                id: chat.id.into(),
                kind: "private".to_string(),
                username: chat.username,
                first_name: Some(chat.first_name),
                last_name: chat.last_name,
                title: None,
                invite_link: None,
            },
            MessageChat::Group(chat) => NewTelegramChat {
                id: chat.id.into(),
                kind: "group".to_string(),
                title: Some(chat.title),
                username: None,
                first_name: None,
                last_name: None,
                invite_link: chat.invite_link,
            },
            MessageChat::Supergroup(chat) => NewTelegramChat {
                id: chat.id.into(),
                kind: "supergroup".to_string(),
                title: Some(chat.title),
                username: chat.username,
                first_name: None,
                last_name: None,
                invite_link: chat.invite_link,
            },
            MessageChat::Unknown(chat) => NewTelegramChat {
                id: chat.id.into(),
                kind: "unknown".to_string(),
                title: chat.title,
                username: chat.username,
                first_name: chat.first_name,
                last_name: chat.last_name,
                invite_link: chat.invite_link,
            },
        }
    }
}

fn commands_string() -> String {
    format!(
        "{} - show the bot's description and contact information\n\
         {} url - subscribe to feed\n\
         {} url - unsubscribe from feed\n\
         {} - list your subscriptions\n\
         {} - show available commands\n\
         {} - set your timezone. All received dates will be converted to this timezone. It should be offset in minutes from UTC. For example, if you live in UTC +10 timezone, offset is equal to 600\n\
         {} - get your timezone\n",
        START, SUBSCRIBE, UNSUBSCRIBE, LIST_SUBSCRIPTIONS, HELP, SET_TIMEZONE, GET_TIMEZONE
    )
}

async fn help(api: Api, message: Message) -> Result<(), Error> {
    let response = commands_string();

    api.send(message.text_reply(response)).await?;
    Ok(())
}

async fn start(api: Api, message: Message) -> Result<(), Error> {
    let response = format!(
        "El Monitorro is feed reader as a Telegram bot.\n\
         It supports RSS, Atom and JSON feeds.\n\n\
         Available commands:\n\
         {}\n\n\
         Synchronization information.\n\
         When you subscribe to a new feed, you'll receive 10 last messages from it. After that, you'll start receiving only new feed items.\n\
         Feed updates check interval is 1 minute. Unread items delivery interval is also 1 minute.\n\
         Currently, the number of subscriptions is limited to 20.\n\n\
         Contact @Ayrat555 with your feedback, suggestions, found bugs, etc. The bot is open source. You can find it at https://github.com/ayrat555/el_monitorro",
        commands_string()
    );

    api.send(message.text_reply(response)).await?;
    Ok(())
}

pub async fn send_message(chat_id: i64, message: String) -> Result<(), Error> {
    let user_id: UserId = chat_id.into();
    let token = env::var("TELEGRAM_BOT_TOKEN").expect("TELEGRAM_BOT_TOKEN not set");

    let api = Api::new(token);

    api.send(user_id.text(message)).await?;

    Ok(())
}

async fn unknown_command(api: Api, message: Message) -> Result<(), Error> {
    let response = "Unknown command. Use /help to show available commands".to_string();

    api.send(message.text_reply(response)).await?;
    Ok(())
}

async fn subscribe(api: Api, message: Message, data: String) -> Result<(), Error> {
    let response = match logic::create_subscription(
        &db::establish_connection(),
        message.chat.clone().into(),
        Some(data.clone()),
    ) {
        Ok(_subscription) => format!("Successfully subscribed to {}", data),
        Err(SubscriptionError::DbError(_)) => {
            "Something went wrong with the bot's storage".to_string()
        }
        Err(SubscriptionError::InvalidUrl) => "Invalid url".to_string(),
        Err(SubscriptionError::RssUrlNotProvided) => "Url is not provided".to_string(),
        Err(SubscriptionError::UrlIsNotFeed) => "Url is not a feed".to_string(),
        Err(SubscriptionError::SubscriptionAlreadyExists) => {
            "Susbscription already exists".to_string()
        }
        Err(SubscriptionError::SubscriptionCountLimit) => {
            "You exceeded the number of subscriptions".to_string()
        }
        Err(SubscriptionError::TelegramError) => "Something went wrong with Telegram".to_string(),
    };

    api.send(message.text_reply(response)).await?;
    Ok(())
}

async fn unsubscribe(api: Api, message: Message, data: String) -> Result<(), Error> {
    let chat_id = get_chat_id(&message);

    let response =
        match logic::delete_subscription(&db::establish_connection(), chat_id.into(), data.clone())
        {
            Ok(_) => format!("Successfully unsubscribed from {}", data),
            Err(DeleteSubscriptionError::DbError) => format!("Failed to unsubscribe from {}", data),
            _ => "Subscription does not exist".to_string(),
        };

    api.send(message.text_reply(response)).await?;
    Ok(())
}

async fn list_subscriptions(api: Api, message: Message) -> Result<(), Error> {
    let chat_id = get_chat_id(&message);

    let response = logic::find_feeds_by_chat_id(&db::establish_connection(), chat_id.into());

    api.send(message.text_reply(response)).await?;
    Ok(())
}

async fn set_timezone(api: Api, message: Message, data: String) -> Result<(), Error> {
    let chat_id = get_chat_id(&message);

    let response = match logic::set_timezone(&db::establish_connection(), chat_id, data) {
        Ok(_) => "Your timezone was updated".to_string(),
        Err(err_string) => err_string.to_string(),
    };

    api.send(message.text_reply(response)).await?;
    Ok(())
}

async fn get_timezone(api: Api, message: Message) -> Result<(), Error> {
    let chat_id = get_chat_id(&message);

    let response = logic::get_timezone(&db::establish_connection(), chat_id);

    api.send(message.text_reply(response)).await?;
    Ok(())
}

async fn process(api: Api, message: Message) -> Result<(), Error> {
    match message.kind {
        MessageKind::Text { ref data, .. } => {
            let command = data.as_str();

            log::info!("{:?} wrote: {}", get_chat_id(&message), command);

            if command.contains(SUBSCRIBE) {
                let argument = parse_argument(command, SUBSCRIBE);
                tokio::spawn(subscribe(api, message, argument));
            } else if command.contains(LIST_SUBSCRIPTIONS) {
                tokio::spawn(list_subscriptions(api, message));
            } else if command.contains(UNSUBSCRIBE) {
                let argument = parse_argument(command, UNSUBSCRIBE);
                tokio::spawn(unsubscribe(api, message, argument));
            } else if command.contains(HELP) {
                tokio::spawn(help(api, message));
            } else if command.contains(START) {
                tokio::spawn(start(api, message));
            } else if command.contains(SET_TIMEZONE) {
                let argument = parse_argument(command, SET_TIMEZONE);
                tokio::spawn(set_timezone(api, message, argument));
            } else if command.contains(GET_TIMEZONE) {
                tokio::spawn(get_timezone(api, message));
            } else {
                tokio::spawn(unknown_command(api, message));
            }
        }
        _ => (),
    };

    Ok(())
}

fn get_chat_id(message: &Message) -> i64 {
    message.chat.id().into()
}

fn parse_argument(full_command: &str, command: &str) -> String {
    full_command.replace(command, "").trim().to_string()
}

pub async fn start_bot() -> Result<(), Error> {
    let token = env::var("TELEGRAM_BOT_TOKEN").expect("TELEGRAM_BOT_TOKEN not set");

    let api = Api::new(token);
    let mut stream = api.stream();

    log::info!("Starting a bot");

    while let Some(update) = stream.next().await {
        let update = update?;
        if let UpdateKind::Message(message) = update.kind {
            tokio::spawn(process(api.clone(), message));
        }
    }

    Ok(())
}
