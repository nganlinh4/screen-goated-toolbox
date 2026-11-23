use super::*;
use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/app-update/fallback-selection.json"
    )))
    .unwrap()
}

fn release(fixture: &Value, name: &str) -> Value {
    let descriptor = &fixture["releases"][name];
    let defaults = &fixture["defaults"];
    let version = descriptor["version"].as_str().unwrap();
    let mut release = crate::updater::parity_fixture::fixture()["latest_release"].clone();
    release["tag_name"] = json!(format!("v{version}"));
    release["body"] = descriptor["body"].clone();
    release["draft"] = json!(descriptor["draft"].as_bool().unwrap_or(false));
    for asset in release["assets"].as_array_mut().unwrap() {
        let extension = if asset["name"].as_str().unwrap().ends_with(".exe") {
            "exe"
        } else {
            "apk"
        };
        let platform = if extension == "exe" { "windows" } else { "apk" };
        let filename = format!("ScreenGoatedToolbox_v{version}.{extension}");
        asset["name"] = json!(filename);
        asset["browser_download_url"] = json!(format!(
            "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/v{version}/{filename}"
        ));
        let size_field = format!("{platform}_size");
        asset["size"] = descriptor
            .get(&size_field)
            .unwrap_or(&defaults[&size_field])
            .clone();
        let hash_field = format!("{platform}_sha256");
        asset["digest"] = json!(format!(
            "sha256:{}",
            descriptor
                .get(&hash_field)
                .unwrap_or(&defaults[&hash_field])
                .as_str()
                .unwrap()
        ));
    }
    release
}

fn contract(candidate: &UpdateCandidate) -> Value {
    json!({"version":candidate.version.to_string(),"body":candidate.body,
        "name":candidate.asset_name,"url":candidate.download_url,"size":candidate.size_bytes,"sha256":candidate.sha256})
}

#[test]
fn shared_fallback_selection_requires_a_complete_unambiguous_highest_release() {
    let fixture = fixture();
    assert_eq!(fixture["limits"]["page_size"], RELEASES_PER_PAGE);
    assert_eq!(fixture["limits"]["maximum_pages"], MAX_RELEASE_PAGES);
    let mut failures = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let pages: Vec<_> = case["pages"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|page| {
                std::iter::repeat_n(page.clone(), page["repeat"].as_u64().unwrap_or(1) as usize)
            })
            .collect();
        let mut requests = Vec::new();
        let result = fetch_latest_with(|page| {
            requests.push(page);
            let page = pages.get(page - 1).context("unexpected release page")?;
            if page["http_status"].as_u64().unwrap_or(200) != 200 {
                anyhow::bail!("owned page request failed");
            }
            let mut releases = Vec::new();
            for entry in page["entries"].as_array().unwrap() {
                for _ in 0..entry["count"].as_u64().unwrap() {
                    releases.push(release(&fixture, entry["release"].as_str().unwrap()));
                }
            }
            serde_json::from_value(json!(releases)).context("owned page metadata was invalid")
        });
        let accepted = case["windows"].as_bool().unwrap();
        let selected_matches = !accepted
            || result.as_ref().ok().map(contract)
                == candidate_from_release(
                    serde_json::from_value(release(&fixture, case["selected"].as_str().unwrap()))
                        .unwrap(),
                )
                .as_ref()
                .map(contract);
        let expected_requests: Vec<_> = (1..=case["requests"].as_u64().unwrap() as usize).collect();
        if result.is_ok() != accepted || !selected_matches || requests != expected_requests {
            failures.push(format!(
                "{}:accepted={},pages={requests:?},candidate={result:?}",
                case["name"].as_str().unwrap(),
                result.is_ok()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
