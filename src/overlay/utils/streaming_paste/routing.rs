//! Routes the unfinished segment across destinations without moving old editor text.
use super::policy::Event;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
pub(super) struct Routing {
    previous: String,
    cut: usize,
}

impl Routing {
    pub fn detach(&mut self) {
        self.cut = self.previous.len();
    }

    pub fn project(&self, event: &Event) -> (Event, usize) {
        let (Event::Interim(text) | Event::Final(text)) = event else {
            return (Event::Finish, 0);
        };
        let cut = mapped_cut(&self.previous, text, self.cut);
        let tail = text[cut..].to_owned();
        let projected = match event {
            Event::Interim(_) => Event::Interim(tail),
            _ => Event::Final(tail),
        };
        (projected, cut)
    }

    pub fn accept(&mut self, event: &Event, cut: usize, replaceable: bool) {
        match event {
            Event::Interim(text) if replaceable => {
                self.previous.clone_from(text);
                self.cut = cut;
            }
            Event::Final(_) | Event::Finish => *self = Self::default(),
            _ => {}
        }
    }
}

fn mapped_cut(old: &str, new: &str, cut: usize) -> usize {
    if cut == 0 {
        return 0;
    }
    let left: Vec<_> = old.split_word_bounds().collect();
    let right: Vec<_> = new.split_word_bounds().collect();
    let equal = left.iter().zip(&right).take_while(|(a, b)| a == b).count();
    let prefix: usize = left[..equal].iter().map(|s| s.len()).sum();
    if cut <= prefix {
        return cut;
    }
    let suffix: usize = left[equal..]
        .iter()
        .rev()
        .zip(right[equal..].iter().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len())
        .sum();
    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    if cut >= old_end && old_end < old.len() {
        new_end + (cut - old_end)
    } else {
        // A correction crossing the destination boundary belongs to the new
        // destination. Never discard new speech or rewrite the abandoned editor.
        prefix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_routing_contract() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/preset-system/destination-routing.json"
        )))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let mut route = Routing::default();
            for step in case["events"].as_array().unwrap() {
                if step["kind"] == "switch" {
                    route.detach();
                    continue;
                }
                let text = step["text"].as_str().unwrap().to_owned();
                let event = if step["kind"] == "final" {
                    Event::Final(text)
                } else {
                    Event::Interim(text)
                };
                let (projected, cut) = route.project(&event);
                let expected = step["tail"].as_str().unwrap();
                assert_eq!(projected.chars(), expected.chars().count(), "{case}");
                assert_eq!(
                    projected,
                    match event {
                        Event::Final(_) => Event::Final(expected.into()),
                        _ => Event::Interim(expected.into()),
                    },
                    "{case}"
                );
                route.accept(&event, cut, true);
            }
        }
    }

    #[test]
    fn unacknowledged_projection_and_append_preview_do_not_consume_speech() {
        let mut route = Routing::default();
        let event = Event::Interim("pending".into());
        route.project(&event);
        route.accept(&event, 0, false);
        route.detach();
        assert_eq!(
            route.project(&Event::Final("pending".into())).0,
            Event::Final("pending".into())
        );
    }

    #[test]
    fn uncertain_attempt_is_not_replayed_but_new_tail_does_not_wait_for_final() {
        let mut route = Routing::default();
        route.accept(&Event::Interim("attempted words".into()), 0, true);
        route.detach();
        let next = Event::Interim("attempted words keep coming".into());
        let (projected, cut) = route.project(&next);
        assert_eq!(projected, Event::Interim(" keep coming".into()));
        route.accept(&next, cut, true);
        route.detach();
        assert_eq!(
            route
                .project(&Event::Interim("attempted words keep coming now".into()))
                .0,
            Event::Interim(" now".into())
        );
    }
}
