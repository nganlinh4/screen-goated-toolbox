#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InteractiveRequestWorkload {
    pub encoded_request_bytes: u64,
    pub expected_response_bytes: u64,
}

impl InteractiveRequestWorkload {
    pub fn for_input(text_bytes: usize, image_bytes: usize) -> Self {
        Self {
            encoded_request_bytes: (text_bytes as u64)
                .saturating_add((image_bytes as u64).div_ceil(3).saturating_mul(4)),
            expected_response_bytes: 0,
        }
    }
}
