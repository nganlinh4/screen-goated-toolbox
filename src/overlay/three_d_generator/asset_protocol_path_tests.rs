use super::*;

#[cfg(windows)]
#[test]
fn model_validation_rejects_a_same_directory_symlink() {
    use std::os::windows::fs::symlink_file;

    let root = std::env::temp_dir().join(format!(
        "sgt-glb-same-directory-link-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let target = root.join("target.glb");
    let linked = root.join("result.glb");
    let geometry = br#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}]}"#;
    std::fs::write(&target, super::tests::glb_bytes(geometry, &[0; 36])).unwrap();
    if symlink_file(&target, &linked).is_err() {
        std::fs::remove_dir_all(root).unwrap();
        return;
    }
    assert!(validate_generated(linked.to_string_lossy().as_ref(), &root).is_err());
    std::fs::remove_file(linked).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
