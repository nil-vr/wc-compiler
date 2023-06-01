use base64::prelude::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{digest::Output, Sha256};

#[derive(Default, Deserialize, Serialize)]
pub struct State {
    pub posters: Vec<Poster>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Poster {
    pub last_used: DateTime<Utc>,
    #[serde(
        serialize_with = "serialize_hash",
        deserialize_with = "deserialize_hash"
    )]
    pub sha256: Output<Sha256>,
}

fn serialize_hash<S>(hash: &Output<Sha256>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&BASE64_STANDARD.encode(&hash[..]))
}

fn deserialize_hash<'d, D>(deserializer: D) -> Result<Output<Sha256>, D::Error>
where
    D: Deserializer<'d>,
{
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Output<Sha256>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "an SHA-256 hash")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            let mut hash = Output::<Sha256>::default();
            // `decode_slice` initially gets the size wrong and refuses to decode into a correctly
            // sized buffer…
            let mut buffer = [0; 33];
            let len = BASE64_STANDARD
                .decode_slice(v, &mut buffer)
                .map_err(E::custom)?;
            if len != hash[..].len() {
                return Err(E::custom("Unexpected hash length"));
            }
            hash.copy_from_slice(&buffer[..len]);
            Ok(hash)
        }
    }
    deserializer.deserialize_str(Visitor)
}
