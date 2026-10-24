//! Pure pre-order logic: Dodo checkout session body, webhook verification and event filtering.
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;

pub const AGENCY_PRODUCT_ID: &str = "pdt_0NpivoyuFRZUf0fvEylOg";
pub const PERSONAL_PRODUCT_ID: &str = "pdt_0NqDoRZUoz8LY0PbCgNu8";
pub const FOUNDING_SPOTS: u64 = 50;
const TOLERANCE_SECS: i64 = 5 * 60;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Plan {
    #[default]
    Agency,
    Personal,
}

impl Plan {
    pub fn parse(s: &str) -> Option<Plan> {
        match s {
            "agency" => Some(Plan::Agency),
            "personal" => Some(Plan::Personal),
            _ => None,
        }
    }

    pub fn product_id(self) -> &'static str {
        match self {
            Plan::Agency => AGENCY_PRODUCT_ID,
            Plan::Personal => PERSONAL_PRODUCT_ID,
        }
    }

    fn from_product(id: &str) -> Option<Plan> {
        [Plan::Agency, Plan::Personal]
            .into_iter()
            .find(|p| p.product_id() == id)
    }

    fn return_url(self) -> &'static str {
        match self {
            Plan::Agency => "https://virling.com/agency?preorder=done",
            Plan::Personal => "https://virling.com/pricing?preorder=done",
        }
    }

    fn source(self) -> &'static str {
        match self {
            Plan::Agency => "virling.com/agency",
            Plan::Personal => "virling.com/pricing",
        }
    }

    /// KV key of the founder counter; the agency key predates plans and stays as it was.
    pub fn count_key(self) -> &'static str {
        match self {
            Plan::Agency => "founders:count",
            Plan::Personal => "founders:personal:count",
        }
    }

    pub fn founder_key(self, payment_id: &str) -> String {
        match self {
            Plan::Agency => format!("founder:{payment_id}"),
            Plan::Personal => format!("founder:personal:{payment_id}"),
        }
    }

    pub fn conversion(self) -> &'static str {
        match self {
            Plan::Agency => "preorder",
            Plan::Personal => "preorder_personal",
        }
    }
}

pub fn session_body(
    plan: Plan,
    email: Option<&str>,
    visitor_id: Option<&str>,
    code: Option<&str>,
) -> Value {
    let mut body = json!({
        "product_cart": [{ "product_id": plan.product_id(), "quantity": 1 }],
        "return_url": plan.return_url(),
        "metadata": { "source": plan.source() },
        "feature_flags": {
            "redirect_immediately": true,
            "allow_discount_code": true,
            "allow_phone_number_collection": false,
            "allow_tax_id": false
        },
        "customization": {
            "theme": "light",
            "theme_config": {
                "radius": "10px",
                "light": {
                    "bg_primary": "#FFFFFF",
                    "border_primary": "#D6D7D1",
                    "text_primary": "#0E1116",
                    "text_secondary": "#4B515C",
                    "input_focus_border": "#0E1116",
                    "button_primary": "#0E1116",
                    "button_primary_hover": "#23272F",
                    "button_text_primary": "#FFFFFF"
                }
            }
        }
    });
    if let Some(email) = email {
        body["customer"] = json!({ "email": email });
    }
    if let Some(vid) = visitor_id {
        body["metadata"]["virling_vid"] = json!(vid);
    }
    if let Some(code) = code {
        body["discount_codes"] = json!([code]);
    }
    body
}

/// The `virling_vid` cookie from a `Cookie` header, for when the page script didn't send one.
pub fn cookie_visitor(header: &str) -> Option<String> {
    header
        .split(';')
        .find_map(|c| c.trim().strip_prefix("virling_vid="))
        .map(str::to_string)
}

#[derive(Debug, PartialEq, Eq)]
pub enum VerifyError {
    Malformed,
    Stale,
    Mismatch,
}

pub fn verify_signature(
    secret: &str,
    id: &str,
    timestamp: &str,
    body: &str,
    signatures: &str,
    now: i64,
) -> Result<(), VerifyError> {
    let ts: i64 = timestamp.parse().map_err(|_| VerifyError::Malformed)?;
    if (now - ts).abs() > TOLERANCE_SECS {
        return Err(VerifyError::Stale);
    }
    let key = STANDARD
        .decode(secret.strip_prefix("whsec_").unwrap_or(secret))
        .map_err(|_| VerifyError::Malformed)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(&key).map_err(|_| VerifyError::Malformed)?;
    mac.update(format!("{id}.{timestamp}.{body}").as_bytes());
    let valid = signatures
        .split_whitespace()
        .filter_map(|s| s.strip_prefix("v1,"))
        .filter_map(|s| STANDARD.decode(s).ok())
        .any(|sig| mac.clone().verify_slice(&sig).is_ok());
    if valid {
        Ok(())
    } else {
        Err(VerifyError::Mismatch)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Founder {
    pub plan: Plan,
    pub payment_id: String,
    pub email: String,
    pub name: Option<String>,
    pub visitor_id: Option<String>,
    pub amount_cents: Option<i64>,
}

#[derive(Deserialize)]
struct Event {
    #[serde(rename = "type")]
    kind: String,
    data: Data,
}

#[derive(Deserialize)]
struct Data {
    payment_id: Option<String>,
    #[serde(default)]
    product_cart: Vec<CartItem>,
    customer: Option<Customer>,
    total_amount: Option<i64>,
    metadata: Option<Value>,
}

#[derive(Deserialize)]
struct CartItem {
    product_id: String,
}

#[derive(Deserialize)]
struct Customer {
    email: String,
    name: Option<String>,
}

/// Returns the new founder when the event is a succeeded payment for a founding spot.
pub fn founder_from_event(body: &str) -> Option<Founder> {
    let event: Event = serde_json::from_str(body).ok()?;
    if event.kind != "payment.succeeded" {
        return None;
    }
    let plan = event
        .data
        .product_cart
        .iter()
        .find_map(|i| Plan::from_product(&i.product_id))?;
    let customer = event.data.customer?;
    Some(Founder {
        plan,
        payment_id: event.data.payment_id?,
        email: customer.email,
        name: customer.name.filter(|n| !n.is_empty()),
        visitor_id: event
            .data
            .metadata
            .as_ref()
            .and_then(|m| m["virling_vid"].as_str())
            .map(str::to_string),
        amount_cents: event.data.total_amount,
    })
}

/// The plan of a founder record from its KV key, for the founder lookup.
pub fn plan_of_key(key: &str) -> Option<Plan> {
    if key.starts_with("founder:personal:") {
        Some(Plan::Personal)
    } else if key.starts_with("founder:") {
        Some(Plan::Agency)
    } else {
        None
    }
}

pub fn spots_left(founders: u64) -> u64 {
    FOUNDING_SPOTS.saturating_sub(founders)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";

    fn sign(id: &str, ts: &str, body: &str) -> String {
        let key = STANDARD.decode(&SECRET[6..]).unwrap();
        let mut mac = Hmac::<Sha256>::new_from_slice(&key).unwrap();
        mac.update(format!("{id}.{ts}.{body}").as_bytes());
        format!("v1,{}", STANDARD.encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn accepts_standard_webhooks_test_vector() {
        let body = r#"{"test": 2432232314}"#;
        let sig = "v1,g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE=";
        assert_eq!(
            verify_signature(
                SECRET,
                "msg_p5jXN8AQM9LWM0D4loKWxJek",
                "1614265330",
                body,
                sig,
                1614265330
            ),
            Ok(())
        );
    }

    #[test]
    fn accepts_any_matching_signature_in_list() {
        let sig = format!("v1,AAAA v2,zzz {}", sign("msg_1", "1700000000", "{}"));
        assert_eq!(
            verify_signature(SECRET, "msg_1", "1700000000", "{}", &sig, 1700000100),
            Ok(())
        );
    }

    #[test]
    fn rejects_tampered_body_and_wrong_id() {
        let sig = sign("msg_1", "1700000000", r#"{"a":1}"#);
        assert_eq!(
            verify_signature(
                SECRET,
                "msg_1",
                "1700000000",
                r#"{"a":2}"#,
                &sig,
                1700000000
            ),
            Err(VerifyError::Mismatch)
        );
        assert_eq!(
            verify_signature(
                SECRET,
                "msg_2",
                "1700000000",
                r#"{"a":1}"#,
                &sig,
                1700000000
            ),
            Err(VerifyError::Mismatch)
        );
    }

    #[test]
    fn rejects_old_and_future_timestamps() {
        let sig = sign("msg_1", "1700000000", "{}");
        assert_eq!(
            verify_signature(SECRET, "msg_1", "1700000000", "{}", &sig, 1700000301),
            Err(VerifyError::Stale)
        );
        assert_eq!(
            verify_signature(SECRET, "msg_1", "1700000000", "{}", &sig, 1699999699),
            Err(VerifyError::Stale)
        );
        assert_eq!(
            verify_signature(SECRET, "msg_1", "nope", "{}", &sig, 1700000000),
            Err(VerifyError::Malformed)
        );
    }

    #[test]
    fn rejects_missing_signature() {
        assert_eq!(
            verify_signature(SECRET, "msg_1", "1700000000", "{}", "", 1700000000),
            Err(VerifyError::Mismatch)
        );
    }

    fn event(kind: &str, product: &str) -> String {
        json!({
            "business_id": "bus_1",
            "type": kind,
            "timestamp": "2026-10-14T10:00:00Z",
            "data": {
                "payload_type": "Payment",
                "payment_id": "pay_123",
                "product_cart": [{ "product_id": product, "quantity": 1 }],
                "status": "succeeded",
                "customer": { "customer_id": "cus_1", "email": "a@agency.com", "name": "Ann" },
                "total_amount": 2900,
                "currency": "USD",
                "metadata": { "source": "virling.com/agency", "virling_vid": "vid-12345678" }
            }
        })
        .to_string()
    }

    #[test]
    fn picks_succeeded_payment_for_founding_product() {
        assert_eq!(
            founder_from_event(&event("payment.succeeded", AGENCY_PRODUCT_ID)),
            Some(Founder {
                plan: Plan::Agency,
                payment_id: "pay_123".into(),
                email: "a@agency.com".into(),
                name: Some("Ann".into()),
                visitor_id: Some("vid-12345678".into()),
                amount_cents: Some(2900),
            })
        );
    }

    #[test]
    fn picks_the_plan_from_the_product() {
        let founder = founder_from_event(&event("payment.succeeded", PERSONAL_PRODUCT_ID)).unwrap();
        assert_eq!(founder.plan, Plan::Personal);
        assert_eq!(founder.payment_id, "pay_123");
        assert_eq!(
            founder_from_event(&event("payment.succeeded", AGENCY_PRODUCT_ID))
                .unwrap()
                .plan,
            Plan::Agency
        );
        assert_ne!(AGENCY_PRODUCT_ID, PERSONAL_PRODUCT_ID);
    }

    #[test]
    fn ignores_other_events_and_products() {
        assert_eq!(
            founder_from_event(&event("payment.failed", PERSONAL_PRODUCT_ID)),
            None
        );
        assert_eq!(
            founder_from_event(&event("subscription.active", AGENCY_PRODUCT_ID)),
            None
        );
        assert_eq!(
            founder_from_event(&event("payment.succeeded", "pdt_other")),
            None
        );
        assert_eq!(
            founder_from_event(r#"{"type":"payment.succeeded","data":{"payload_type":"Payment"}}"#),
            None
        );
        assert_eq!(founder_from_event("not json"), None);
    }

    #[test]
    fn session_body_prefills_email_only_when_given() {
        let body = session_body(
            Plan::Agency,
            Some("a@agency.com"),
            Some("vid-12345678"),
            Some("VIRLINGTEST96"),
        );
        assert_eq!(body["product_cart"][0]["product_id"], AGENCY_PRODUCT_ID);
        assert_eq!(body["product_cart"][0]["quantity"], 1);
        assert_eq!(body["customer"]["email"], "a@agency.com");
        assert_eq!(body["metadata"]["source"], "virling.com/agency");
        assert_eq!(body["metadata"]["virling_vid"], "vid-12345678");
        assert_eq!(body["discount_codes"][0], "VIRLINGTEST96");
        assert_eq!(body["feature_flags"]["allow_discount_code"], true);
        let bare = session_body(Plan::Agency, None, None, None);
        assert!(bare.get("customer").is_none());
        assert!(bare["metadata"].get("virling_vid").is_none());
        assert!(bare.get("discount_codes").is_none());
    }

    #[test]
    fn session_body_follows_the_plan() {
        let agency = session_body(Plan::Agency, None, None, None);
        assert_eq!(
            agency["return_url"],
            "https://virling.com/agency?preorder=done"
        );
        let personal = session_body(Plan::Personal, Some("a@me.com"), Some("vid-12345678"), None);
        assert_eq!(
            personal["product_cart"][0]["product_id"],
            PERSONAL_PRODUCT_ID
        );
        assert_eq!(personal["product_cart"][0]["quantity"], 1);
        assert_eq!(
            personal["return_url"],
            "https://virling.com/pricing?preorder=done"
        );
        assert_eq!(personal["metadata"]["source"], "virling.com/pricing");
        assert_eq!(personal["metadata"]["virling_vid"], "vid-12345678");
        assert_eq!(personal["customer"]["email"], "a@me.com");
    }

    #[test]
    fn plans_parse_and_default_to_agency() {
        assert_eq!(Plan::default(), Plan::Agency);
        assert_eq!(Plan::parse("agency"), Some(Plan::Agency));
        assert_eq!(Plan::parse("personal"), Some(Plan::Personal));
        assert_eq!(Plan::parse("team"), None);
        assert_eq!(
            serde_json::from_str::<Plan>(r#""personal""#).unwrap(),
            Plan::Personal
        );
        assert!(serde_json::from_str::<Plan>(r#""team""#).is_err());
    }

    #[test]
    fn each_plan_keeps_its_own_counter_and_records() {
        assert_eq!(Plan::Agency.count_key(), "founders:count");
        assert_eq!(Plan::Agency.founder_key("pay_1"), "founder:pay_1");
        assert_eq!(Plan::Personal.count_key(), "founders:personal:count");
        assert_eq!(
            Plan::Personal.founder_key("pay_1"),
            "founder:personal:pay_1"
        );
        assert_eq!(Plan::Agency.conversion(), "preorder");
        assert_eq!(Plan::Personal.conversion(), "preorder_personal");
    }

    #[test]
    fn reads_visitor_from_cookie_header() {
        assert_eq!(
            cookie_visitor("a=1; virling_vid=vabc123; b=2").as_deref(),
            Some("vabc123")
        );
        assert_eq!(cookie_visitor("a=1"), None);
    }

    #[test]
    fn founder_keys_name_their_plan() {
        assert_eq!(
            plan_of_key(&Plan::Agency.founder_key("pay_1")),
            Some(Plan::Agency)
        );
        assert_eq!(
            plan_of_key(&Plan::Personal.founder_key("pay_1")),
            Some(Plan::Personal)
        );
        assert_eq!(plan_of_key(Plan::Agency.count_key()), None);
        assert_eq!(plan_of_key("signup:agency:a@b.co"), None);
    }

    #[test]
    fn spots_never_go_negative() {
        assert_eq!(spots_left(0), 50);
        assert_eq!(spots_left(49), 1);
        assert_eq!(spots_left(60), 0);
        assert_eq!(spots_left(FOUNDING_SPOTS), 0);
    }
}
