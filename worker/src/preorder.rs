//! Pure pre-order logic: Dodo checkout session body, webhook verification and event filtering.
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::Sha256;

pub const PRODUCT_ID: &str = "pdt_0Npg3jPouv8ghbW4s5Sxd";
pub const FOUNDING_SPOTS: u64 = 50;
const TOLERANCE_SECS: i64 = 5 * 60;

pub fn session_body(email: Option<&str>, visitor_id: Option<&str>, code: Option<&str>) -> Value {
    let mut body = json!({
        "product_cart": [{ "product_id": PRODUCT_ID, "quantity": 1 }],
        "return_url": "https://virling.com/agency?preorder=done",
        "metadata": { "source": "virling.com/agency" },
        "feature_flags": { "redirect_immediately": true, "allow_discount_code": true },
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
    pub subscription_id: String,
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
    subscription_id: Option<String>,
    product_id: Option<String>,
    customer: Option<Customer>,
    recurring_pre_tax_amount: Option<i64>,
    metadata: Option<Value>,
}

#[derive(Deserialize)]
struct Customer {
    email: String,
    name: Option<String>,
}

/// Returns the new founder when the event is an activated subscription to the founding product.
pub fn founder_from_event(body: &str) -> Option<Founder> {
    let event: Event = serde_json::from_str(body).ok()?;
    if event.kind != "subscription.active" || event.data.product_id.as_deref() != Some(PRODUCT_ID) {
        return None;
    }
    let customer = event.data.customer?;
    Some(Founder {
        subscription_id: event.data.subscription_id?,
        email: customer.email,
        name: customer.name.filter(|n| !n.is_empty()),
        visitor_id: event
            .data
            .metadata
            .as_ref()
            .and_then(|m| m["virling_vid"].as_str())
            .map(str::to_string),
        amount_cents: event.data.recurring_pre_tax_amount,
    })
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
                "payload_type": "Subscription",
                "subscription_id": "sub_123",
                "product_id": product,
                "status": "active",
                "customer": { "customer_id": "cus_1", "email": "a@agency.com", "name": "Ann" },
                "recurring_pre_tax_amount": 2900,
                "currency": "USD",
                "metadata": { "source": "virling.com/agency", "virling_vid": "vid-12345678" }
            }
        })
        .to_string()
    }

    #[test]
    fn picks_active_subscription_for_founding_product() {
        assert_eq!(
            founder_from_event(&event("subscription.active", PRODUCT_ID)),
            Some(Founder {
                subscription_id: "sub_123".into(),
                email: "a@agency.com".into(),
                name: Some("Ann".into()),
                visitor_id: Some("vid-12345678".into()),
                amount_cents: Some(2900),
            })
        );
    }

    #[test]
    fn ignores_other_events_and_products() {
        assert_eq!(
            founder_from_event(&event("subscription.renewed", PRODUCT_ID)),
            None
        );
        assert_eq!(
            founder_from_event(&event("payment.succeeded", PRODUCT_ID)),
            None
        );
        assert_eq!(
            founder_from_event(&event("subscription.active", "pdt_other")),
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
            Some("a@agency.com"),
            Some("vid-12345678"),
            Some("VIRLINGTEST96"),
        );
        assert_eq!(body["product_cart"][0]["product_id"], PRODUCT_ID);
        assert_eq!(body["product_cart"][0]["quantity"], 1);
        assert_eq!(body["customer"]["email"], "a@agency.com");
        assert_eq!(body["metadata"]["source"], "virling.com/agency");
        assert_eq!(body["metadata"]["virling_vid"], "vid-12345678");
        assert_eq!(body["discount_codes"][0], "VIRLINGTEST96");
        assert_eq!(body["feature_flags"]["allow_discount_code"], true);
        let bare = session_body(None, None, None);
        assert!(bare.get("customer").is_none());
        assert!(bare["metadata"].get("virling_vid").is_none());
        assert!(bare.get("discount_codes").is_none());
    }

    #[test]
    fn spots_never_go_negative() {
        assert_eq!(spots_left(0), 50);
        assert_eq!(spots_left(49), 1);
        assert_eq!(spots_left(60), 0);
    }
}
