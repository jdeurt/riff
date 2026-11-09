//! Seeded semantic defaults and format generation shared by every binding.
use chrono::{DateTime, SecondsFormat};
use fake::Fake;
use fake::faker::address::raw::{CityName, CountryCode};
use fake::faker::company::raw::CompanyName;
use fake::faker::impls::address::CityNameGenFn;
use fake::faker::internet::raw::FreeEmail;
use fake::faker::name::raw::Name;
use fake::faker::phone_number::raw::PhoneNumber;
use fake::locales::{DE_DE, Data, EN, FR_FR, JA_JP};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde_json::Value;

use crate::{Error, ProviderContext, ProviderKind, Result};

pub const LOCALES: &[&str] = &["en-US", "en-GB", "de-DE", "fr-FR", "ja-JP"];
pub const FORMATS: &[&str] = &[
    "email",
    "idn-email",
    "hostname",
    "idn-hostname",
    "uri",
    "uri-reference",
    "iri",
    "iri-reference",
    "uri-template",
    "uuid",
    "date",
    "date-time",
    "time",
    "duration",
    "ipv4",
    "ipv6",
    "json-pointer",
    "relative-json-pointer",
    "regex",
    "color",
];

pub fn generate(context: &ProviderContext) -> Result<Value> {
    let mut random = ChaCha8Rng::seed_from_u64(context.seed);
    let value = match context.kind {
        ProviderKind::Format => formatted(&context.name, &mut random)?,
        ProviderKind::Provider => match context.locale.as_str() {
            "en-US" | "en-GB" => semantic(&context.name, EN, &mut random)?,
            "de-DE" => semantic(&context.name, DE_DE, &mut random)?,
            "fr-FR" => semantic(&context.name, FR_FR, &mut random)?,
            "ja-JP" => semantic(&context.name, JA_JP, &mut random)?,
            locale => return Err(Error::Provider(format!("Unsupported locale: {locale}"))),
        },
    };
    Ok(Value::String(value))
}

fn semantic<L: Data + Copy + CityNameGenFn>(
    name: &str,
    locale: L,
    random: &mut impl Rng,
) -> Result<String> {
    Ok(match name {
        "email" => FreeEmail(locale).fake_with_rng(random),
        "full-name" => Name(locale).fake_with_rng(random),
        "company-name" => CompanyName(locale).fake_with_rng(random),
        "phone-number" => PhoneNumber(locale).fake_with_rng(random),
        "city" => CityName(locale).fake_with_rng(random),
        "country-code" => CountryCode(locale).fake_with_rng(random),
        "url" => format!("https://fixture-{}.example.test/", random.random::<u32>()),
        "uuid" => uuid::Builder::from_random_bytes(random.random()).into_uuid().to_string(),
        _ => return Err(Error::Provider(format!("Unknown provider: {name}"))),
    })
}

fn formatted(name: &str, random: &mut impl Rng) -> Result<String> {
    Ok(match name {
        "email" | "idn-email" => return semantic("email", EN, random),
        "uuid" => return semantic("uuid", EN, random),
        "uri" | "uri-reference" | "iri" | "iri-reference" => return semantic("url", EN, random),
        "hostname" | "idn-hostname" => format!("fixture-{}.example.test", random.random::<u32>()),
        "ipv4" => std::net::Ipv4Addr::from(random.random::<u32>()).to_string(),
        "ipv6" => std::net::Ipv6Addr::from(random.random::<u128>()).to_string(),
        "date" => timestamp(random)?.format("%Y-%m-%d").to_string(),
        "date-time" => timestamp(random)?.to_rfc3339_opts(SecondsFormat::Secs, true),
        "time" => timestamp(random)?.format("%H:%M:%SZ").to_string(),
        "duration" => format!("P{}D", random.random_range(0..366)),
        "json-pointer" => "/name".to_owned(),
        "relative-json-pointer" => "0/name".to_owned(),
        "uri-template" => "/customers/{id}".to_owned(),
        "regex" => "[A-Z]+".to_owned(),
        "color" => "#a0b1c2".to_owned(),
        _ => return Err(Error::Provider(format!("Unsupported string format: {name}"))),
    })
}

fn timestamp(random: &mut impl Rng) -> Result<DateTime<chrono::Utc>> {
    // Fixed UTC interval 2000-01-01 through 2030-01-01; replay never reads the clock.
    const START_SECONDS: i64 = 946_684_800;
    const INTERVAL_SECONDS: i64 = 946_771_200;
    DateTime::from_timestamp(START_SECONDS + random.random_range(0..INTERVAL_SECONDS), 0)
        .ok_or_else(|| {
            Error::Generation("Generated timestamp is outside the UTC range.".to_owned())
        })
}
