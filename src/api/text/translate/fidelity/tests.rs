use super::*;

/// The source that produced every observed failure, kept verbatim.
const MIXED_SOURCE: &str = "Use UltraSEEK 검사 운영.\n\nIt is short and broad enough to cover analysis, review, report issuance, and delivery without tying the project to only polyp detection.";

#[test]
fn an_ordinary_translation_passes() {
    let reply = "Sử dụng UltraSEEK để vận hành kiểm tra.\n\nNó ngắn gọn và đủ rộng để bao quát phân tích, xem xét, phát hành báo cáo và giao nộp mà không giới hạn dự án chỉ vào việc phát hiện polyp.";
    assert_eq!(introduced_script(MIXED_SOURCE, reply), None);
}

#[test]
fn a_term_carried_through_from_the_source_passes() {
    // Leaving a proper noun in its original script is a legitimate choice, and
    // several models make it on this input.
    let reply = "Sử dụng UltraSEEK 검사 운영.\n\nNó ngắn gọn và đủ rộng để bao phủ phân tích, báo cáo và giao hàng, không chỉ phát hiện polyp.";
    assert_eq!(introduced_script(MIXED_SOURCE, reply), None);
}

#[test]
fn a_stray_character_fused_onto_a_word_is_caught() {
    // Observed: `polyp đơn독`. The source contains Hangul, but never this
    // character, so allowing the script wholesale would have missed it.
    let reply = "Đây là công cụ ngắn gọn đủ để bao phủ phân tích, đánh giá, báo cáo và giao nhận mà không gắn với việc phát hiện polyp đơn독.";
    assert_eq!(introduced_script(MIXED_SOURCE, reply), Some("독"));
}

#[test]
fn a_reply_that_drifts_into_another_language_is_caught() {
    // Observed: Portuguese, Italian, Spanish and Tamil in one Vietnamese reply.
    // The Latin fragments are indistinguishable from Vietnamese by script alone;
    // the Tamil is what makes this decidable.
    let reply =
        "UVK Kiểm tra trực tuyến, phân tích báo cáo. impegnò Vamos de Spada, acabando en இகணர்பு";
    assert!(introduced_script(MIXED_SOURCE, reply).is_some());
}

#[test]
fn translating_into_another_script_is_not_mistaken_for_drift() {
    // The whole reply is Korean because Korean was asked for. Nothing about the
    // target language is configured here; it is inferred from the reply itself.
    let source = "The battery is low. Connect the charger to continue.";
    let reply = "배터리가 부족합니다. 충전기를 연결하세요.";
    assert_eq!(introduced_script(source, reply), None);

    let reply = "電源ボタンを長押ししてください。";
    assert_eq!(
        introduced_script("Press and hold the power button.", reply),
        None
    );
}

#[test]
fn a_single_foreign_character_inside_a_translation_into_another_script_is_caught() {
    // The mirror of the Hangul case: a Korean reply carrying a stray Tamil run.
    let source = "The battery is low.";
    let reply = "배터리가 இகணர் 부족합니다.";
    assert_eq!(introduced_script(source, reply), Some("இகணர்"));
}

#[test]
fn shared_characters_carry_no_evidence() {
    // Numbers, punctuation and whitespace appear in every language and must not
    // be read as a script change.
    let source = "Order 12,345 ships on 2026-08-21 — confirm?";
    let reply = "Đơn hàng 12,345 giao ngày 2026-08-21 — xác nhận?";
    assert_eq!(introduced_script(source, reply), None);
}

#[test]
fn an_empty_or_unscripted_reply_is_not_judged() {
    assert_eq!(introduced_script("anything", ""), None);
    assert_eq!(introduced_script("anything", "12345 — 67%"), None);
}

#[test]
fn a_multi_character_term_absent_from_the_source_is_caught_whole() {
    let source = "Use the scanner.";
    let reply = "Sử dụng máy quét 検査運用 để kiểm tra.";
    assert_eq!(introduced_script(source, reply), Some("検査運用"));
}
