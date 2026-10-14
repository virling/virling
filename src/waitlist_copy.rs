//! Waitlist confirmation email, shared with the Cloudflare Worker in worker/.
use askama::Template;

pub struct Copy {
    pub subject: &'static str,
    pub headline: &'static str,
    pub intro: &'static str,
    pub steps: [&'static str; 3],
    pub offer: &'static str,
    pub question: &'static str,
}

pub const SELFHOST: Copy = Copy {
    subject: "You\u{2019}re on the Virling waitlist",
    headline: "You\u{2019}re on the list.",
    intro: "Thanks for joining. Virling shows which channels bring paying customers, not just visitors, and runs as one Rust binary you can host yourself.",
    steps: [
        "Build updates every couple of weeks, nothing more.",
        "Early access to the self-host binary and the hosted beta.",
        "You pick: run it yourself for free, or let us host it.",
    ],
    offer: "Self-hosting is free forever. Hosted is $19/month; the first 50 customers get 40% off for life ($11.40/month).",
    question: "What do you use for analytics today, and how do you figure out which channel made you money?",
};

pub const AGENCY: Copy = Copy {
    subject: "You\u{2019}re on the list for Virling for agencies",
    headline: "You\u{2019}re on the list. Let\u{2019}s set up your first client.",
    intro: "Thanks for joining. Virling sends each of your clients a weekly report, under your brand, showing which channels brought paying customers.",
    steps: [
        "We\u{2019}ll reach out personally within a few days.",
        "We set up your first client together, about ten minutes.",
        "Your first branded report goes out the next Monday.",
    ],
    offer: "Founding price: $29 a month instead of $79, for life, for the first 50 agencies.",
    question: "How many clients do you report to, and what do those reports show today?",
};

#[derive(Template)]
#[template(path = "email/waitlist.html")]
pub struct ConfirmEmail<'a> {
    pub copy: &'a Copy,
    pub leave_link: &'a str,
}

pub fn confirm_text(copy: &Copy, leave_link: &str) -> String {
    format!(
        "{}\n\n{}\n\nWhat happens next\n01  {}\n02  {}\n03  {}\n\n{}\n\nOne question: {} Just hit reply. We read every answer.\n\nDeven and Rahul\nBuilding Virling\n\nLeave the waitlist: {}\n",
        copy.headline,
        copy.intro,
        copy.steps[0],
        copy.steps[1],
        copy.steps[2],
        copy.offer,
        copy.question,
        leave_link
    )
}
