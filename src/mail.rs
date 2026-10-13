use lettre::message::{Mailbox, MultiPart, SinglePart, header::ContentType};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

/// Sends mail over SMTP (Cloudflare Email Service, SES, Postmark, ...) or logs it when no SMTP URL is set.
pub enum Mailer {
    Smtp {
        transport: Box<AsyncSmtpTransport<Tokio1Executor>>,
        from: Mailbox,
    },
    Log,
    #[cfg(test)]
    Capture(std::sync::Mutex<Vec<(String, String, String)>>),
}

impl Mailer {
    /// `url` like `smtps://api_token:TOKEN@smtp.mx.cloudflare.net:465`.
    pub fn new(url: Option<&str>, from: &str) -> anyhow::Result<Self> {
        match url {
            Some(url) => Ok(Mailer::Smtp {
                transport: Box::new(AsyncSmtpTransport::<Tokio1Executor>::from_url(url)?.build()),
                from: from.parse()?,
            }),
            None => Ok(Mailer::Log),
        }
    }

    pub async fn send(
        &self,
        to: &str,
        subject: &str,
        text: String,
        html: String,
    ) -> anyhow::Result<()> {
        self.send_as(None, to, subject, text, html).await
    }

    /// Sends from `from` (replies go there too) instead of the default sender.
    pub async fn send_as(
        &self,
        from: Option<&str>,
        to: &str,
        subject: &str,
        text: String,
        html: String,
    ) -> anyhow::Result<()> {
        match self {
            Mailer::Smtp {
                transport,
                from: default_from,
            } => {
                let sender: Mailbox = match from {
                    Some(f) => f.parse()?,
                    None => default_from.clone(),
                };
                let message = Message::builder()
                    .from(sender.clone())
                    .reply_to(sender)
                    .to(to.parse()?)
                    .subject(subject)
                    .multipart(
                        MultiPart::alternative()
                            .singlepart(
                                SinglePart::builder()
                                    .header(ContentType::TEXT_PLAIN)
                                    .body(text),
                            )
                            .singlepart(
                                SinglePart::builder()
                                    .header(ContentType::TEXT_HTML)
                                    .body(html),
                            ),
                    )?;
                transport.send(message).await?;
            }
            Mailer::Log => {
                tracing::info!(%to, %subject, "no SMTP configured, email not sent:\n{text}")
            }
            #[cfg(test)]
            Mailer::Capture(sent) => {
                sent.lock()
                    .unwrap()
                    .push((to.to_string(), subject.to_string(), text))
            }
        }
        Ok(())
    }
}
