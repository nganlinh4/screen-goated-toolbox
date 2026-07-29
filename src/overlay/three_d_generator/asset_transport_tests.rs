use super::model_asset_url;

#[test]
fn model_asset_url_matches_the_platform_custom_protocol_transport() {
    let url = model_asset_url("opaque-token");
    if cfg!(windows) {
        assert_eq!(url, "http://sgt3d.localhost/model/opaque-token");
    } else {
        assert_eq!(url, "sgt3d://localhost/model/opaque-token");
    }
}
