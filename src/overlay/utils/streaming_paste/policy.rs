//! Platform-independent ownership policy for revisable streaming output.

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Event {
    Interim(String),
    Final(String),
    Finish,
}

#[derive(Debug, PartialEq)]
pub(super) struct Replacement {
    pub old: String,
    pub new: String,
}

pub(super) struct Policy {
    replaceable: bool,
    tail: String,
    suspended: bool,
    finished: bool,
}

impl Policy {
    pub fn new(replaceable: bool) -> Self {
        Self {
            replaceable,
            tail: String::new(),
            suspended: false,
            finished: false,
        }
    }

    pub fn suspend(&mut self) {
        self.suspended = true;
    }

    pub fn plan(&self, event: &Event) -> Option<Replacement> {
        if self.suspended || self.finished {
            return None;
        }
        let text = match event {
            Event::Interim(text) if self.replaceable => text,
            Event::Interim(_) => return None,
            Event::Final(text) => text,
            Event::Finish => "",
        };
        let new = sanitize(text);
        (new != self.tail).then(|| Replacement {
            old: self.tail.clone(),
            new,
        })
    }

    pub fn accept(&mut self, event: &Event) {
        if self.suspended || self.finished {
            return;
        }
        match event {
            Event::Interim(text) if self.replaceable => self.tail = sanitize(text),
            Event::Final(_) => self.tail.clear(),
            Event::Finish => {
                self.tail.clear();
                self.finished = true;
            }
            _ => {}
        }
    }
}

pub(super) fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') {
                ' '
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_provisional_paste_contract() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/preset-system/provisional-paste.json"
        )))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let mut policy = Policy::new(case["replaceable"].as_bool().unwrap());
            for value in case["events"].as_array().unwrap() {
                let text = value["text"].as_str().unwrap_or_default().to_owned();
                let event = match value["kind"].as_str().unwrap() {
                    "interim" => Event::Interim(text),
                    "final" => Event::Final(text),
                    "finish" => Event::Finish,
                    "lost" => {
                        policy.suspend();
                        continue;
                    }
                    _ => panic!("invalid fixture event"),
                };
                let expected = value["new"].as_str().map(|new| Replacement {
                    old: value["old"].as_str().unwrap().to_owned(),
                    new: new.to_owned(),
                });
                assert_eq!(policy.plan(&event), expected, "{}: {event:?}", case["name"]);
                policy.accept(&event);
            }
        }
    }

    #[test]
    fn same_text_final_commits_without_retyping_and_next_equal_final_is_distinct() {
        let mut policy = Policy::new(true);
        policy.accept(&Event::Interim("yes".into()));
        assert_eq!(policy.plan(&Event::Final("yes".into())), None);
        policy.accept(&Event::Final("yes".into()));
        assert_eq!(policy.plan(&Event::Final("yes".into())).unwrap().new, "yes");
    }

    #[test]
    fn closed_or_uncertain_session_never_restarts_or_erases() {
        for uncertain in [false, true] {
            let mut policy = Policy::new(true);
            policy.accept(&Event::Interim("draft".into()));
            if uncertain {
                policy.suspend();
            } else {
                policy.accept(&Event::Finish);
            }
            for event in [
                Event::Interim("late".into()),
                Event::Final("late".into()),
                Event::Finish,
            ] {
                assert_eq!(policy.plan(&event), None);
            }
        }
    }
}
