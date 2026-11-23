use anyhow::Result;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{UploadTarget, file_lease::FileLease};

impl UploadTarget {
    pub(super) fn proposal(&self, tab_id: i64, selector: &str, file: &FileLease) -> Result<Value> {
        Ok(json!({
            "capability_class":"local_file_disclosure", "operation":"assign_file_to_browser_document",
            "source": file.proposal()?,
            "receiver": self.receiver_proposal(tab_id, selector)?,
            "effect":"The receiving page can read or transfer the file immediately after assignment."
        }))
    }

    fn receiver_proposal(&self, tab_id: i64, selector: &str) -> Result<Value> {
        let mut location = url::Url::parse(&self.url)?;
        let display_url = if location.cannot_be_a_base() {
            format!("{}:<opaque>", location.scheme())
        } else {
            if !location.username().is_empty() {
                location
                    .set_username("")
                    .map_err(|_| anyhow::anyhow!("could not redact receiver credentials"))?;
            }
            if location.password().is_some() {
                location
                    .set_password(None)
                    .map_err(|_| anyhow::anyhow!("could not redact receiver credentials"))?;
            }
            location.set_query(None);
            location.set_fragment(None);
            location.to_string()
        };
        Ok(json!({
            "tab_id":tab_id,"document_id":self.document_id,
                "origin":self.origin,"url":display_url,
                "location_identity":format!("{:x}", Sha256::digest(self.url.as_bytes())),
                "selector":selector,"backend_node_id":self.backend_node_id
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(url: &str) -> UploadTarget {
        UploadTarget {
            document_id: "document-a".into(),
            backend_node_id: 29,
            node_id: 9,
            url: url.into(),
            origin: "https://receiver.example".into(),
        }
    }

    #[test]
    fn receiver_metadata_redacts_credentials_query_and_fragment_but_binds_full_location() {
        let original = target("https://user:password@receiver.example/upload?token=secret#private");
        let proposal = original.receiver_proposal(3, "input").unwrap();
        assert_eq!(proposal["url"], "https://receiver.example/upload");
        let serialized = proposal.to_string();
        for secret in ["user:", "password", "token", "secret", "private"] {
            assert!(!serialized.contains(secret));
        }
        let changed = target("https://user:password@receiver.example/upload?token=other#private")
            .receiver_proposal(3, "input")
            .unwrap();
        assert_eq!(proposal["url"], changed["url"]);
        assert_ne!(proposal["location_identity"], changed["location_identity"]);
    }

    #[test]
    fn opaque_receiver_location_never_becomes_checker_content() {
        let proposal = target("data:text/html,private-payload?secret#fragment")
            .receiver_proposal(3, "input")
            .unwrap();
        assert_eq!(proposal["url"], "data:<opaque>");
        assert!(!proposal.to_string().contains("private-payload"));
        assert!(target("not a URL").receiver_proposal(3, "input").is_err());
    }
}
