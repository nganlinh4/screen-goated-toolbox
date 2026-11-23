use std::borrow::Cow;
use std::sync::LazyLock;

use wry::RequestAsyncResponder;
use wry::http::Request;

use super::background::Lane;

type Pending = (Request<Vec<u8>>, RequestAsyncResponder);

static LANE: LazyLock<Option<Lane<Pending>>> = LazyLock::new(|| {
    Lane::new("sgt-model-preview", 8, |(request, responder): Pending| {
        responder.respond(super::asset_protocol::handle(request));
    })
    .inspect_err(|error| crate::log_info!("[3D Generator] Model previews unavailable: {error}"))
    .ok()
});

pub(super) fn request(request: Request<Vec<u8>>, responder: RequestAsyncResponder) {
    let pending = (request, responder);
    let rejected = if pending.0.body().len() > 1024 {
        Some(pending)
    } else if let Some(lane) = LANE.as_ref() {
        lane.submit(pending).err()
    } else {
        Some(pending)
    };
    if let Some((_, responder)) = rejected {
        responder.respond(super::asset_protocol::response(
            503,
            Cow::Borrowed(b"Model preview is busy."),
        ));
    }
}
