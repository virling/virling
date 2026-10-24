//! Throwaway mailbox domains that can't sign in, because nobody reads them after the first link.

const DOMAINS: &[&str] = &[
    "mailinator.com",
    "guerrillamail.com",
    "guerrillamail.net",
    "guerrillamail.org",
    "guerrillamail.biz",
    "guerrillamail.de",
    "guerrillamailblock.com",
    "sharklasers.com",
    "grr.la",
    "spam4.me",
    "10minutemail.com",
    "10minutemail.net",
    "20minutemail.com",
    "tempmail.com",
    "temp-mail.org",
    "temp-mail.io",
    "tempmailo.com",
    "tempmail.net",
    "tempail.com",
    "tempr.email",
    "tempinbox.com",
    "tempemail.net",
    "gettempmail.com",
    "throwawaymail.com",
    "yopmail.com",
    "yopmail.fr",
    "yopmail.net",
    "trashmail.com",
    "trashmail.net",
    "trashmail.de",
    "getnada.com",
    "nada.email",
    "maildrop.cc",
    "dispostable.com",
    "fakeinbox.com",
    "mailnesia.com",
    "mintemail.com",
    "mohmal.com",
    "emailondeck.com",
    "burnermail.io",
    "discard.email",
    "spamgourmet.com",
    "mytemp.email",
    "moakt.com",
    "mail.tm",
    "1secmail.com",
    "1secmail.org",
    "1secmail.net",
    "esiix.com",
    "wwjmp.com",
    "xojxe.com",
    "yoggm.com",
    "vjuum.com",
    "laafd.com",
    "txcct.com",
    "dropmail.me",
    "harakirimail.com",
    "inboxkitten.com",
    "tmpmail.org",
    "tmpmail.net",
    "emailfake.com",
    "fakemail.net",
    "fakemailgenerator.com",
    "mailcatch.com",
    "mailsac.com",
    "mvrht.net",
    "linshiyouxiang.net",
    "spambox.us",
    "spam.la",
    "jetable.org",
    "anonbox.net",
    "cuvox.de",
    "armyspy.com",
    "dayrep.com",
    "einrot.com",
    "fleckens.hu",
    "gustr.com",
    "jourrapide.com",
    "rhyta.com",
    "superrito.com",
    "teleworm.us",
    "bccto.me",
    "luxusmail.org",
    "maileater.com",
    "mailforspam.com",
    "mailnull.com",
    "meltmail.com",
    "notmailinator.com",
    "owlpic.com",
    "pokemail.net",
    "quickinbox.com",
    "receiveee.com",
    "reallymymail.com",
    "safetymail.info",
    "mailtemp.info",
    "temporary-mail.net",
    "tmail.ws",
    "mail7.io",
    "mailpoof.com",
    "emltmp.com",
    "mailbox.in.ua",
    "disposablemail.com",
    "throwam.com",
    "trashmail.io",
    "trash-mail.com",
    "wegwerfmail.de",
    "wegwerfmail.net",
];

/// True when the address is on a throwaway domain or a subdomain of one.
pub fn is_disposable(email: &str) -> bool {
    let Some((_, domain)) = email.rsplit_once('@') else {
        return false;
    };
    let domain = domain.trim().to_ascii_lowercase();
    DOMAINS
        .iter()
        .any(|d| domain == *d || domain.ends_with(&format!(".{d}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_throwaway_domains_and_their_subdomains() {
        assert!(is_disposable("a@mailinator.com"));
        assert!(is_disposable("a@Sub.YopMail.net"));
        assert!(is_disposable("a@x.mailinator.com"));
        assert!(!is_disposable("a@gmail.com"));
        assert!(!is_disposable("a@notmailinator.org"));
        assert!(!is_disposable("nobody"));
    }
}
